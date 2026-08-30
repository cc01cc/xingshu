use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};

use crate::types::{
    FetchLog, Policy, RepoKind, RepoRecord, Root, Tag, TaskRecord, TaskRepoRecord, TaskRepoUpdate,
    TaskUpdate, VcsError,
};

const SCHEMA: &str = include_str!("../../../schema.sql");
const SCHEMA_VERSION: i64 = 4;

#[derive(Debug, Clone)]
pub struct Database {
    path: PathBuf,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, VcsError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| VcsError::Filesystem(error.to_string()))?;
        }
        let database = Self { path };
        database.with_connection(|connection| {
            connection
                .execute_batch(SCHEMA)
                .map_err(|error| VcsError::Database(error.to_string()))?;
            ensure_column(
                connection,
                "tasks",
                "conflict_mode",
                "TEXT NOT NULL DEFAULT 'policy'",
            )?;
            let now = Utc::now().to_rfc3339();
            connection
                .execute(
                    "INSERT OR IGNORE INTO schema_meta (version, applied_at, created_at, updated_at)
                     VALUES (?1, ?2, ?2, ?2)",
                    params![SCHEMA_VERSION, now],
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            Ok(())
        })?;
        Ok(database)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn with_connection<T>(
        &self,
        operation: impl FnOnce(&Connection) -> Result<T, VcsError>,
    ) -> Result<T, VcsError> {
        let connection =
            Connection::open(&self.path).map_err(|error| VcsError::Database(error.to_string()))?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(|error| VcsError::Database(error.to_string()))?;
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .map_err(|error| VcsError::Database(error.to_string()))?;
        operation(&connection)
    }

    pub fn upsert_root(&self, root: &Root) -> Result<i64, VcsError> {
        let now = Utc::now().to_rfc3339();
        self.with_connection(|connection| {
            connection
                .execute(
                    "INSERT INTO roots (name, path, disk_label, mount_point, priority, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                     ON CONFLICT(path) DO UPDATE SET name=excluded.name, disk_label=excluded.disk_label,
                       mount_point=excluded.mount_point, priority=excluded.priority, updated_at=excluded.updated_at",
                    params![
                        root.name,
                        root.path.to_string_lossy(),
                        root.disk_label,
                        root.mount_point,
                        root.priority,
                        now,
                    ],
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            connection
                .query_row(
                    "SELECT id FROM roots WHERE path = ?1",
                    params![root.path.to_string_lossy()],
                    |row| row.get(0),
                )
                .map_err(|error| VcsError::Database(error.to_string()))
        })
    }

    pub fn root_id_for_path(&self, path: &Path) -> Result<Option<i64>, VcsError> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT id FROM roots WHERE path = ?1",
                    params![path.to_string_lossy()],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| VcsError::Database(error.to_string()))
        })
    }

    pub fn list_roots(&self) -> Result<Vec<Root>, VcsError> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT id, name, path, disk_label, mount_point, priority, created_at, updated_at
                     FROM roots ORDER BY priority, name",
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            let rows = statement
                .query_map([], |row| {
                    Ok(Root {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        path: PathBuf::from(row.get::<_, String>(2)?),
                        disk_label: row.get(3)?,
                        mount_point: row.get(4)?,
                        priority: row.get(5)?,
                        created_at: parse_time(row.get(6)?)?,
                        updated_at: parse_time(row.get(7)?)?,
                    })
                })
                .map_err(|error| VcsError::Database(error.to_string()))?;
            rows.map(|row| row.map_err(|error| VcsError::Database(error.to_string())))
                .collect()
        })
    }

    pub fn remove_root(&self, root_id: i64) -> Result<(), VcsError> {
        self.with_connection(|connection| {
            connection
                .execute("DELETE FROM roots WHERE id = ?1", params![root_id])
                .map_err(|error| VcsError::Database(error.to_string()))?;
            Ok(())
        })
    }

    pub fn upsert_repo(&self, repo: &RepoRecord) -> Result<i64, VcsError> {
        let now = Utc::now().to_rfc3339();
        let default_lock = repo.repo_kind.default_modify_lock();
        self.with_connection(|connection| {
            connection
                .execute(
                    "INSERT INTO repos (root_id, org, name, rel_path, remote_url, default_branch, head_commit,
                       size_bytes, is_bare, clone_status, repo_kind, modify_lock, lock_violation,
                       push_protect_remote, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)
                     ON CONFLICT(root_id, rel_path) DO UPDATE SET org=excluded.org, name=excluded.name,
                       remote_url=excluded.remote_url, default_branch=excluded.default_branch,
                       head_commit=excluded.head_commit, size_bytes=excluded.size_bytes, is_bare=excluded.is_bare,
                       clone_status=excluded.clone_status, lock_violation=excluded.lock_violation,
                       updated_at=excluded.updated_at",
                    params![
                        repo.root_id,
                        repo.org,
                        repo.name,
                        repo.rel_path.to_string_lossy(),
                        repo.remote_url,
                        repo.default_branch,
                        repo.head_commit,
                        repo.size_bytes as i64,
                        repo.is_bare,
                        repo.clone_status,
                        repo.repo_kind.as_str(),
                        if repo.modify_lock { 1 } else { 0 },
                        if repo.lock_violation { 1 } else { 0 },
                        repo.push_protect_remote,
                        now,
                    ],
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;

            let id: i64 = connection
                .query_row(
                    "SELECT id FROM repos WHERE root_id = ?1 AND rel_path = ?2",
                    params![repo.root_id, repo.rel_path.to_string_lossy()],
                    |row| row.get(0),
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;

            if repo.id.is_none() {
                connection
                    .execute(
                        "UPDATE repos SET modify_lock = COALESCE((SELECT modify_lock FROM repos WHERE id = ?1), ?2) WHERE id = ?1",
                        params![id, if default_lock { 1 } else { 0 }],
                    )
                    .map_err(|error| VcsError::Database(error.to_string()))?;
            }
            Ok(id)
        })
    }

    pub fn set_repo_kind(
        &self,
        repo_id: i64,
        kind: &RepoKind,
        upstream_remote: Option<&str>,
    ) -> Result<(), VcsError> {
        self.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE repos SET repo_kind = ?1, modify_lock = ?2, push_protect_remote = ?3, updated_at = ?4 WHERE id = ?5",
                    params![
                        kind.as_str(),
                        if kind.default_modify_lock() { 1 } else { 0 },
                        upstream_remote,
                        Utc::now().to_rfc3339(),
                        repo_id,
                    ],
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            Ok(())
        })
    }

    pub fn set_lock_violation(&self, repo_id: i64, violation: bool) -> Result<(), VcsError> {
        self.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE repos SET lock_violation = ?1, updated_at = ?2 WHERE id = ?3",
                    params![
                        if violation { 1 } else { 0 },
                        Utc::now().to_rfc3339(),
                        repo_id
                    ],
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            Ok(())
        })
    }

    pub fn update_repo_location(
        &self,
        repo_id: i64,
        root_id: i64,
        relative_path: &Path,
    ) -> Result<(), VcsError> {
        self.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE repos SET root_id = ?1, rel_path = ?2, updated_at = ?3 WHERE id = ?4",
                    params![
                        root_id,
                        relative_path.to_string_lossy(),
                        Utc::now().to_rfc3339(),
                        repo_id
                    ],
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            Ok(())
        })
    }

    pub fn list_repos(&self) -> Result<Vec<RepoRecord>, VcsError> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT id, root_id, org, name, rel_path, remote_url, default_branch, head_commit,
                       last_fetch_at, last_pull_status, size_bytes, is_bare, clone_status, repo_kind,
                       modify_lock, lock_violation, push_protect_remote, created_at, updated_at
                     FROM repos ORDER BY org, name",
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            let rows = statement
                .query_map([], row_to_repo)
                .map_err(|error| VcsError::Database(error.to_string()))?;
            rows.map(|row| row.map_err(|error| VcsError::Database(error.to_string())))
                .collect()
        })
    }

    pub fn find_repo(&self, query: &str) -> Result<Option<RepoRecord>, VcsError> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT id, root_id, org, name, rel_path, remote_url, default_branch, head_commit,
                       last_fetch_at, last_pull_status, size_bytes, is_bare, clone_status, repo_kind,
                       modify_lock, lock_violation, push_protect_remote, created_at, updated_at
                     FROM repos WHERE name = ?1 OR org || '/' || name = ?1
                       OR rel_path = ?1 OR replace(rel_path, char(92), '/') = ?1 LIMIT 1",
                    params![query],
                    row_to_repo,
                )
                .optional()
                .map_err(|error| VcsError::Database(error.to_string()))
        })
    }

    pub fn find_repo_by_id(&self, repo_id: i64) -> Result<Option<RepoRecord>, VcsError> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT id, root_id, org, name, rel_path, remote_url, default_branch, head_commit,
                       last_fetch_at, last_pull_status, size_bytes, is_bare, clone_status, repo_kind,
                       modify_lock, lock_violation, push_protect_remote, created_at, updated_at
                     FROM repos WHERE id = ?1",
                    params![repo_id],
                    row_to_repo,
                )
                .optional()
                .map_err(|error| VcsError::Database(error.to_string()))
        })
    }

    pub fn find_root_by_id(&self, root_id: i64) -> Result<Option<Root>, VcsError> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT id, name, path, disk_label, mount_point, priority, created_at, updated_at
                     FROM roots WHERE id = ?1",
                    params![root_id],
                    |row| {
                        Ok(Root {
                            id: row.get(0)?,
                            name: row.get(1)?,
                            path: PathBuf::from(row.get::<_, String>(2)?),
                            disk_label: row.get(3)?,
                            mount_point: row.get(4)?,
                            priority: row.get(5)?,
                            created_at: parse_time(row.get(6)?)?,
                            updated_at: parse_time(row.get(7)?)?,
                        })
                    },
                )
                .optional()
                .map_err(|error| VcsError::Database(error.to_string()))
        })
    }

    pub fn find_tag_by_id(&self, tag_id: i64) -> Result<Option<Tag>, VcsError> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT id, slug, label, color, kind, created_at, updated_at FROM tags WHERE id = ?1",
                    params![tag_id],
                    |row| {
                        Ok(Tag {
                            id: row.get(0)?,
                            slug: row.get(1)?,
                            label: row.get(2)?,
                            color: row.get(3)?,
                            kind: row.get(4)?,
                            created_at: parse_time(row.get(5)?)?,
                            updated_at: parse_time(row.get(6)?)?,
                        })
                    },
                )
                .optional()
                .map_err(|error| VcsError::Database(error.to_string()))
        })
    }

    pub fn delete_tag(&self, tag_id: i64) -> Result<(), VcsError> {
        self.with_connection(|connection| {
            connection
                .execute("DELETE FROM repo_tags WHERE tag_id = ?1", params![tag_id])
                .map_err(|error| VcsError::Database(error.to_string()))?;
            connection
                .execute("DELETE FROM tags WHERE id = ?1", params![tag_id])
                .map_err(|error| VcsError::Database(error.to_string()))?;
            Ok(())
        })
    }

    pub fn count_by_kind(&self) -> Result<Vec<(String, i64)>, VcsError> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT repo_kind, COUNT(*) FROM repos GROUP BY repo_kind ORDER BY repo_kind",
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })
                .map_err(|error| VcsError::Database(error.to_string()))?;
            rows.map(|row| row.map_err(|error| VcsError::Database(error.to_string())))
                .collect()
        })
    }

    pub fn create_tag(
        &self,
        slug: &str,
        label: &str,
        color: Option<&str>,
    ) -> Result<i64, VcsError> {
        let now = Utc::now().to_rfc3339();
        self.with_connection(|connection| {
            connection
                .execute(
                    "INSERT INTO tags (slug, label, color, kind, created_at, updated_at)
                     VALUES (?1, ?2, ?3, 'manual', ?4, ?4)
                     ON CONFLICT(slug) DO UPDATE SET label=excluded.label, color=excluded.color,
                       updated_at=excluded.updated_at",
                    params![slug, label, color, now],
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            connection
                .query_row(
                    "SELECT id FROM tags WHERE slug = ?1",
                    params![slug],
                    |row| row.get(0),
                )
                .map_err(|error| VcsError::Database(error.to_string()))
        })
    }

    pub fn attach_tag(&self, repo_id: i64, tag_id: i64) -> Result<(), VcsError> {
        let now = Utc::now().to_rfc3339();
        self.with_connection(|connection| {
            connection
                .execute(
                    "INSERT INTO repo_tags (repo_id, tag_id, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?3)
                     ON CONFLICT(repo_id, tag_id) DO UPDATE SET updated_at=excluded.updated_at",
                    params![repo_id, tag_id, now],
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            Ok(())
        })
    }

    pub fn detach_tag(&self, repo_id: i64, tag_id: i64) -> Result<(), VcsError> {
        self.with_connection(|connection| {
            connection
                .execute(
                    "DELETE FROM repo_tags WHERE repo_id = ?1 AND tag_id = ?2",
                    params![repo_id, tag_id],
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            Ok(())
        })
    }

    pub fn tag_id(&self, slug: &str) -> Result<Option<i64>, VcsError> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT id FROM tags WHERE slug = ?1",
                    params![slug],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| VcsError::Database(error.to_string()))
        })
    }

    pub fn repo_has_tag(&self, repo_id: i64, slug: &str) -> Result<bool, VcsError> {
        self.with_connection(|connection| {
            let found: Option<i64> = connection
                .query_row(
                    "SELECT 1 FROM repo_tags rt JOIN tags t ON t.id = rt.tag_id
                     WHERE rt.repo_id = ?1 AND t.slug = ?2 LIMIT 1",
                    params![repo_id, slug],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| VcsError::Database(error.to_string()))?;
            Ok(found.is_some())
        })
    }

    pub fn tags_for_repo(&self, repo_id: i64) -> Result<Vec<String>, VcsError> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT t.slug FROM tags t JOIN repo_tags rt ON rt.tag_id = t.id
                     WHERE rt.repo_id = ?1 ORDER BY t.slug",
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            let rows = statement
                .query_map(params![repo_id], |row| row.get(0))
                .map_err(|error| VcsError::Database(error.to_string()))?;
            rows.map(|row| row.map_err(|error| VcsError::Database(error.to_string())))
                .collect()
        })
    }

    pub fn list_tags(&self) -> Result<Vec<Tag>, VcsError> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare("SELECT id, slug, label, color, kind, created_at, updated_at FROM tags ORDER BY label")
                .map_err(|error| VcsError::Database(error.to_string()))?;
            let rows = statement
                .query_map([], |row| {
                    Ok(Tag {
                        id: row.get(0)?,
                        slug: row.get(1)?,
                        label: row.get(2)?,
                        color: row.get(3)?,
                        kind: row.get(4)?,
                        created_at: parse_time(row.get(5)?)?,
                        updated_at: parse_time(row.get(6)?)?,
                    })
                })
                .map_err(|error| VcsError::Database(error.to_string()))?;
            rows.map(|row| row.map_err(|error| VcsError::Database(error.to_string())))
                .collect()
        })
    }

    pub fn set_repo_policy(&self, policy: &Policy) -> Result<i64, VcsError> {
        let repo_id = policy
            .repo_id
            .ok_or_else(|| VcsError::Database("repo policy requires repo_id".to_owned()))?;
        let now = Utc::now().to_rfc3339();
        self.with_connection(|connection| {
            connection
                .execute("DELETE FROM policies WHERE repo_id = ?1", params![repo_id])
                .map_err(|error| VcsError::Database(error.to_string()))?;
            connection
                .execute(
                    "INSERT INTO policies (repo_id, tag_id, pull_strategy, fetch_schedule, depth, auto_tag,
                       pull_conflict_policy, unattended_conflict_policy, created_at, updated_at)
                     VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                    params![
                        repo_id,
                        policy.pull_strategy,
                        policy.fetch_schedule,
                        policy.depth,
                        if policy.auto_tag { 1 } else { 0 },
                        policy.pull_conflict_policy,
                        policy.unattended_conflict_policy,
                        now,
                    ],
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            Ok(connection.last_insert_rowid())
        })
    }

    pub fn repo_policy(&self, repo_id: i64) -> Result<Option<Policy>, VcsError> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT id, repo_id, tag_id, pull_strategy, fetch_schedule, depth, auto_tag,
                       pull_conflict_policy, unattended_conflict_policy, created_at, updated_at
                     FROM policies WHERE repo_id = ?1 LIMIT 1",
                    params![repo_id],
                    row_to_policy,
                )
                .optional()
                .map_err(|error| VcsError::Database(error.to_string()))
        })
    }

    pub fn set_tag_policy(&self, policy: &Policy) -> Result<i64, VcsError> {
        let tag_id = policy
            .tag_id
            .ok_or_else(|| VcsError::Database("tag policy requires tag_id".to_owned()))?;
        let now = Utc::now().to_rfc3339();
        self.with_connection(|connection| {
            connection
                .execute("DELETE FROM policies WHERE tag_id = ?1", params![tag_id])
                .map_err(|error| VcsError::Database(error.to_string()))?;
            connection
                .execute(
                    "INSERT INTO policies (repo_id, tag_id, pull_strategy, fetch_schedule, depth, auto_tag,
                       pull_conflict_policy, unattended_conflict_policy, created_at, updated_at)
                     VALUES (NULL, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                    params![
                        tag_id,
                        policy.pull_strategy,
                        policy.fetch_schedule,
                        policy.depth,
                        if policy.auto_tag { 1 } else { 0 },
                        policy.pull_conflict_policy,
                        policy.unattended_conflict_policy,
                        now,
                    ],
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            Ok(connection.last_insert_rowid())
        })
    }

    pub fn tag_policy(&self, tag_id: i64) -> Result<Option<Policy>, VcsError> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT id, repo_id, tag_id, pull_strategy, fetch_schedule, depth, auto_tag,
                       pull_conflict_policy, unattended_conflict_policy, created_at, updated_at
                     FROM policies WHERE tag_id = ?1 LIMIT 1",
                    params![tag_id],
                    row_to_policy,
                )
                .optional()
                .map_err(|error| VcsError::Database(error.to_string()))
        })
    }

    pub fn tag_ids_for_repo(&self, repo_id: i64) -> Result<Vec<i64>, VcsError> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare("SELECT tag_id FROM repo_tags WHERE repo_id = ?1")
                .map_err(|error| VcsError::Database(error.to_string()))?;
            let rows = statement
                .query_map(params![repo_id], |row| row.get(0))
                .map_err(|error| VcsError::Database(error.to_string()))?;
            rows.map(|row| row.map_err(|error| VcsError::Database(error.to_string())))
                .collect()
        })
    }

    pub fn create_task(&self, task: &TaskRecord) -> Result<(), VcsError> {
        self.with_connection(|connection| {
            connection
                .execute(
                    "INSERT INTO tasks (id, type, status, request_id, operation_id, progress_current,
                        conflict_mode, progress_total, last_repo, last_result, error, result_json, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)",
                    params![
                        task.id,
                        task.task_type,
                        task.status,
                        task.request_id,
                        task.operation_id,
                        task.progress_current.map(|value| value as i64),
                        task.conflict_mode,
                        task.progress_total.map(|value| value as i64),
                        task.last_repo,
                        task.last_result,
                        task.error,
                        task.result_json,
                        task.created_at.to_rfc3339(),
                    ],
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            Ok(())
        })
    }

    pub fn create_task_repos(&self, task_id: &str, repo_ids: &[i64]) -> Result<(), VcsError> {
        let now = Utc::now().to_rfc3339();
        self.with_connection(|connection| {
            for repo_id in repo_ids {
                connection
                    .execute(
                        "INSERT OR IGNORE INTO task_repos
                         (task_id, repo_id, status, created_at, updated_at)
                         VALUES (?1, ?2, 'pending', ?3, ?3)",
                        params![task_id, repo_id, now],
                    )
                    .map_err(|error| VcsError::Database(error.to_string()))?;
            }
            Ok(())
        })
    }

    pub fn list_task_repos(&self, task_id: &str) -> Result<Vec<TaskRepoRecord>, VcsError> {
        self.with_connection(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT id, task_id, repo_id, status, conflict_reason, requested_action,
                        result, duration_ms, backup_path, error, created_at, updated_at
                     FROM task_repos WHERE task_id = ?1 ORDER BY id",
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            let rows = statement
                .query_map(params![task_id], row_to_task_repo)
                .map_err(|error| VcsError::Database(error.to_string()))?;
            rows.map(|row| row.map_err(|error| VcsError::Database(error.to_string())))
                .collect()
        })
    }

    pub fn find_task_repo(
        &self,
        task_id: &str,
        repo_id: i64,
    ) -> Result<Option<TaskRepoRecord>, VcsError> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT id, task_id, repo_id, status, conflict_reason, requested_action,
                        result, duration_ms, backup_path, error, created_at, updated_at
                     FROM task_repos WHERE task_id = ?1 AND repo_id = ?2",
                    params![task_id, repo_id],
                    row_to_task_repo,
                )
                .optional()
                .map_err(|error| VcsError::Database(error.to_string()))
        })
    }

    pub fn claim_task_repo(
        &self,
        task_id: &str,
        repo_id: i64,
        from_status: &str,
        to_status: &str,
        requested_action: Option<&str>,
    ) -> Result<bool, VcsError> {
        self.with_connection(|connection| {
            let changed = connection
                .execute(
                    "UPDATE task_repos SET status = ?1, requested_action = ?2, updated_at = ?3
                     WHERE task_id = ?4 AND repo_id = ?5 AND status = ?6",
                    params![
                        to_status,
                        requested_action,
                        Utc::now().to_rfc3339(),
                        task_id,
                        repo_id,
                        from_status,
                    ],
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            Ok(changed == 1)
        })
    }

    pub fn update_task_repo(
        &self,
        task_id: &str,
        repo_id: i64,
        update: &TaskRepoUpdate,
    ) -> Result<(), VcsError> {
        self.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE task_repos SET status = ?1, conflict_reason = ?2,
                        requested_action = ?3, result = ?4, duration_ms = ?5,
                        backup_path = ?6, error = ?7, updated_at = ?8
                     WHERE task_id = ?9 AND repo_id = ?10",
                    params![
                        update.status,
                        update.conflict_reason,
                        update.requested_action,
                        update.result,
                        update.duration_ms.map(|value| value as i64),
                        update.backup_path,
                        update.error,
                        Utc::now().to_rfc3339(),
                        task_id,
                        repo_id,
                    ],
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            Ok(())
        })
    }

    pub fn has_active_task_type(&self, task_type: &str) -> Result<bool, VcsError> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT EXISTS(
                         SELECT 1 FROM tasks
                         WHERE type = ?1 AND status IN ('pending', 'running', 'waiting_for_decision')
                     )",
                    params![task_type],
                    |row| row.get(0),
                )
                .map_err(|error| VcsError::Database(error.to_string()))
        })
    }

    pub fn mark_active_task_repos_interrupted(&self) -> Result<(), VcsError> {
        self.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE task_repos SET status = 'interrupted',
                        error = CASE WHEN status = 'resolving'
                            THEN 'server restarted; manual reconciliation required'
                            ELSE COALESCE(error, 'server restarted') END,
                        updated_at = ?1
                     WHERE status IN ('pending', 'running', 'resolving')",
                    params![Utc::now().to_rfc3339()],
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            Ok(())
        })
    }

    pub fn find_task(&self, task_id: &str) -> Result<Option<TaskRecord>, VcsError> {
        self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT id, type, status, conflict_mode, request_id, operation_id, progress_current,
                        progress_total, last_repo, last_result, error, result_json, created_at, updated_at
                     FROM tasks WHERE id = ?1",
                    params![task_id],
                    row_to_task,
                )
                .optional()
                .map_err(|error| VcsError::Database(error.to_string()))
        })
    }

    pub fn update_task(&self, task_id: &str, update: &TaskUpdate) -> Result<(), VcsError> {
        self.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE tasks SET status = ?1, progress_current = ?2, progress_total = ?3,
                       last_repo = COALESCE(?4, last_repo), last_result = COALESCE(?5, last_result),
                       error = ?6, result_json = COALESCE(?7, result_json), updated_at = ?8 WHERE id = ?9",
                    params![
                        update.status,
                        update.current.map(|value| value as i64),
                        update.total.map(|value| value as i64),
                        update.last_repo,
                        update.last_result,
                        update.error,
                        update.result_json,
                        Utc::now().to_rfc3339(),
                        task_id,
                    ],
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            Ok(())
        })
    }

    pub fn mark_active_tasks_interrupted(&self) -> Result<(), VcsError> {
        self.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE tasks SET status = 'interrupted', error = 'server restarted', updated_at = ?1
                     WHERE status IN ('pending', 'running')",
                    params![Utc::now().to_rfc3339()],
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            Ok(())
        })
    }

    pub fn record_fetch_log(&self, log: &FetchLog) -> Result<i64, VcsError> {
        self.with_connection(|connection| {
            connection
                .execute(
                    "INSERT INTO fetch_log (repo_id, started_at, finished_at, strategy, result, objects, bytes,
                       error, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
                    params![
                        log.repo_id,
                        log.started_at.to_rfc3339(),
                        log.finished_at.map(|value| value.to_rfc3339()),
                        log.strategy,
                        log.result,
                        log.objects.map(|value| value as i64),
                        log.bytes.map(|value| value as i64),
                        log.error,
                        log.created_at.to_rfc3339(),
                    ],
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            Ok(connection.last_insert_rowid())
        })
    }

    pub fn update_pull_status(&self, repo_id: i64, status: &str) -> Result<(), VcsError> {
        self.with_connection(|connection| {
            connection
                .execute(
                    "UPDATE repos SET last_pull_status = ?1, last_fetch_at = ?2, updated_at = ?2 WHERE id = ?3",
                    params![status, Utc::now().to_rfc3339(), repo_id],
                )
                .map_err(|error| VcsError::Database(error.to_string()))?;
            Ok(())
        })
    }
}

fn ensure_column(
    connection: &rusqlite::Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), VcsError> {
    let exists = {
        let mut statement = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .map_err(|error| VcsError::Database(error.to_string()))?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| VcsError::Database(error.to_string()))?;
        columns
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| VcsError::Database(error.to_string()))?
            .into_iter()
            .any(|name| name == column)
    };
    if !exists {
        connection
            .execute(
                &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
                [],
            )
            .map_err(|error| VcsError::Database(error.to_string()))?;
    }
    Ok(())
}

fn parse_time(value: String) -> Result<DateTime<Utc>, rusqlite::Error> {
    DateTime::parse_from_rfc3339(&value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|_| {
            rusqlite::Error::InvalidColumnType(
                0,
                "timestamp".to_owned(),
                rusqlite::types::Type::Text,
            )
        })
}

fn parse_optional_time(value: Option<String>) -> Result<Option<DateTime<Utc>>, rusqlite::Error> {
    value.map(parse_time).transpose()
}

fn row_to_repo(row: &rusqlite::Row<'_>) -> Result<RepoRecord, rusqlite::Error> {
    let kind: String = row.get(13)?;
    let repo_kind = RepoKind::try_from(kind.as_str())
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    Ok(RepoRecord {
        id: row.get(0)?,
        root_id: row.get(1)?,
        org: row.get(2)?,
        name: row.get(3)?,
        rel_path: PathBuf::from(row.get::<_, String>(4)?),
        remote_url: row.get(5)?,
        default_branch: row.get(6)?,
        head_commit: row.get(7)?,
        last_fetch_at: parse_optional_time(row.get(8)?)?,
        last_pull_status: row.get(9)?,
        size_bytes: row.get::<_, i64>(10)?.max(0) as u64,
        is_bare: row.get::<_, i64>(11)? != 0,
        clone_status: row.get(12)?,
        repo_kind,
        modify_lock: row.get::<_, i64>(14)? != 0,
        lock_violation: row.get::<_, i64>(15)? != 0,
        push_protect_remote: row.get(16)?,
        created_at: parse_time(row.get(17)?)?,
        updated_at: parse_time(row.get(18)?)?,
    })
}

fn row_to_policy(row: &rusqlite::Row<'_>) -> Result<Policy, rusqlite::Error> {
    Ok(Policy {
        id: row.get(0)?,
        repo_id: row.get(1)?,
        tag_id: row.get(2)?,
        pull_strategy: row.get(3)?,
        fetch_schedule: row.get(4)?,
        depth: row.get(5)?,
        auto_tag: row.get::<_, i64>(6)? != 0,
        pull_conflict_policy: row.get(7)?,
        unattended_conflict_policy: row.get(8)?,
        created_at: parse_time(row.get(9)?)?,
        updated_at: parse_time(row.get(10)?)?,
    })
}

fn row_to_task(row: &rusqlite::Row<'_>) -> Result<TaskRecord, rusqlite::Error> {
    Ok(TaskRecord {
        id: row.get(0)?,
        task_type: row.get(1)?,
        status: row.get(2)?,
        conflict_mode: row.get(3)?,
        request_id: row.get(4)?,
        operation_id: row.get(5)?,
        progress_current: row
            .get::<_, Option<i64>>(6)?
            .map(|value| value.max(0) as u64),
        progress_total: row
            .get::<_, Option<i64>>(7)?
            .map(|value| value.max(0) as u64),
        last_repo: row.get(8)?,
        last_result: row.get(9)?,
        error: row.get(10)?,
        result_json: row.get(11)?,
        created_at: parse_time(row.get(12)?)?,
        updated_at: parse_time(row.get(13)?)?,
    })
}

fn row_to_task_repo(row: &rusqlite::Row<'_>) -> Result<TaskRepoRecord, rusqlite::Error> {
    Ok(TaskRepoRecord {
        id: row.get(0)?,
        task_id: row.get(1)?,
        repo_id: row.get(2)?,
        status: row.get(3)?,
        conflict_reason: row.get(4)?,
        requested_action: row.get(5)?,
        result: row.get(6)?,
        duration_ms: row
            .get::<_, Option<i64>>(7)?
            .map(|value| value.max(0) as u64),
        backup_path: row.get(8)?,
        error: row.get(9)?,
        created_at: parse_time(row.get(10)?)?,
        updated_at: parse_time(row.get(11)?)?,
    })
}
