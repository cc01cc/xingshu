use std::io::Write;

use chrono::Utc;
use rusqlite::Connection;
use tempfile::TempDir;
use xingshu_core::types::{RepoKind, RepoRecord, Root, TaskRecord, TaskRepoUpdate};
use xingshu_core::{Database, logging};

#[test]
fn logging_redacts_credentials_and_validates_request_ids() {
    let value = logging::redact(
        "https://token@example.invalid/repo?access_token=secret&pat=another-secret",
    );
    assert!(!value.contains("token@"));
    assert!(!value.contains("secret"));
    assert!(!value.contains("another-secret"));
    assert!(logging::valid_request_id("plan-204_01"));
    assert!(!logging::valid_request_id("bad request id"));
}

#[test]
fn schema_ledger_and_timestamp_columns_survive_database_reopen() {
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("index.db");
    let database = Database::open(&path).expect("open database");
    database
        .with_connection(|connection| {
            let version: i64 = connection
                .query_row("SELECT MAX(version) FROM schema_meta", [], |row| row.get(0))
                .map_err(|error| xingshu_core::types::VcsError::Database(error.to_string()))?;
            assert_eq!(version, 4);
            for table in [
                "roots",
                "repos",
                "tags",
                "repo_tags",
                "policies",
                "fetch_log",
                "tasks",
                "task_repos",
            ] {
                let mut statement = connection
                    .prepare(&format!("PRAGMA table_info({table})"))
                    .map_err(|error| xingshu_core::types::VcsError::Database(error.to_string()))?;
                let columns = statement
                    .query_map([], |row| row.get::<_, String>(1))
                    .map_err(|error| xingshu_core::types::VcsError::Database(error.to_string()))?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| xingshu_core::types::VcsError::Database(error.to_string()))?;
                assert!(columns.iter().any(|column| column == "created_at"));
                assert!(columns.iter().any(|column| column == "updated_at"));
            }
            Ok(())
        })
        .expect("inspect schema");
    drop(database);
    Database::open(&path).expect("reopen database");
}

#[test]
fn active_tasks_are_marked_interrupted_without_touching_terminal_tasks() {
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("index.db");
    let database = Database::open(&path).expect("open database");
    let now = Utc::now();
    for (id, status) in [("active", "running"), ("finished", "completed")] {
        database
            .create_task(&TaskRecord {
                id: id.to_owned(),
                task_type: "scan".to_owned(),
                status: status.to_owned(),
                conflict_mode: "policy".to_owned(),
                request_id: None,
                operation_id: format!("operation-{id}"),
                progress_current: Some(1),
                progress_total: Some(2),
                last_repo: None,
                last_result: None,
                error: None,
                result_json: None,
                created_at: now,
                updated_at: now,
            })
            .expect("create task");
    }

    database
        .mark_active_tasks_interrupted()
        .expect("mark active tasks");
    let active = database
        .find_task("active")
        .expect("find active task")
        .expect("active task");
    let finished = database
        .find_task("finished")
        .expect("find finished task")
        .expect("finished task");
    assert_eq!(active.status, "interrupted");
    assert_eq!(active.error.as_deref(), Some("server restarted"));
    assert_eq!(finished.status, "completed");
}

#[test]
fn task_repo_restart_recovery_preserves_waiting_decisions_and_interrupts_resolving() {
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("index.db");
    let database = Database::open(&path).expect("open database");
    let root_path = temp.path().join("root");
    std::fs::create_dir_all(&root_path).expect("root path");
    let now = Utc::now();
    let root = Root {
        id: None,
        name: "root".to_owned(),
        path: root_path,
        disk_label: None,
        mount_point: None,
        priority: 0,
        created_at: now,
        updated_at: now,
    };
    let root_id = database.upsert_root(&root).expect("root");
    let repo_id = database
        .upsert_repo(&RepoRecord {
            id: None,
            root_id,
            org: "test".to_owned(),
            name: "repo".to_owned(),
            rel_path: std::path::PathBuf::from("repo"),
            remote_url: None,
            default_branch: Some("main".to_owned()),
            head_commit: None,
            last_fetch_at: None,
            last_pull_status: None,
            size_bytes: 0,
            is_bare: false,
            clone_status: "ok".to_owned(),
            repo_kind: RepoKind::Own,
            modify_lock: false,
            lock_violation: false,
            push_protect_remote: None,
            created_at: now,
            updated_at: now,
        })
        .expect("repo");
    let task = TaskRecord {
        id: "resolving-task".to_owned(),
        task_type: "pull".to_owned(),
        status: "running".to_owned(),
        conflict_mode: "ask".to_owned(),
        request_id: None,
        operation_id: "operation-resolving".to_owned(),
        progress_current: Some(0),
        progress_total: Some(1),
        last_repo: None,
        last_result: None,
        error: None,
        result_json: None,
        created_at: now,
        updated_at: now,
    };
    database.create_task(&task).expect("task");
    database
        .create_task_repos(&task.id, &[repo_id])
        .expect("task repo");
    assert!(
        database
            .claim_task_repo(&task.id, repo_id, "pending", "running", None)
            .expect("claim running")
    );
    assert!(
        database
            .claim_task_repo(&task.id, repo_id, "running", "resolving", Some("overwrite"),)
            .expect("claim resolving")
    );

    let waiting_task = TaskRecord {
        id: "waiting-task".to_owned(),
        task_type: "scan".to_owned(),
        status: "waiting_for_decision".to_owned(),
        conflict_mode: "policy".to_owned(),
        request_id: None,
        operation_id: "operation-waiting".to_owned(),
        progress_current: Some(0),
        progress_total: Some(1),
        last_repo: None,
        last_result: None,
        error: None,
        result_json: None,
        created_at: now,
        updated_at: now,
    };
    database.create_task(&waiting_task).expect("waiting task");
    database
        .create_task_repos(&waiting_task.id, &[repo_id])
        .expect("waiting task repo");
    database
        .update_task_repo(
            &waiting_task.id,
            repo_id,
            &TaskRepoUpdate {
                status: "waiting_decision".to_owned(),
                conflict_reason: Some("dirty".to_owned()),
                conflict: None,
                requested_action: None,
                result: Some("conflict".to_owned()),
                duration_ms: Some(10),
                backup_path: None,
                error: None,
            },
        )
        .expect("waiting repo state");

    database
        .mark_active_task_repos_interrupted()
        .expect("interrupt task repos");
    let resolving = database
        .find_task_repo(&task.id, repo_id)
        .expect("find resolving repo")
        .expect("resolving repo");
    let waiting = database
        .find_task_repo(&waiting_task.id, repo_id)
        .expect("find waiting repo")
        .expect("waiting repo");
    assert_eq!(resolving.status, "interrupted");
    assert_eq!(
        resolving.error.as_deref(),
        Some("server restarted; manual reconciliation required")
    );
    assert_eq!(waiting.status, "waiting_decision");
}

#[test]
fn existing_task_table_is_forward_migrated_with_conflict_mode() {
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("legacy.db");
    let now = Utc::now().to_rfc3339();
    let connection = Connection::open(&path).expect("legacy database");
    connection
        .execute_batch(
            &format!(
                "CREATE TABLE schema_meta (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
                 INSERT INTO schema_meta VALUES (3, '{now}', '{now}', '{now}');
                 CREATE TABLE tasks (
                     id TEXT PRIMARY KEY, type TEXT NOT NULL, status TEXT NOT NULL,
                     request_id TEXT, operation_id TEXT NOT NULL, progress_current INTEGER,
                     progress_total INTEGER, last_repo TEXT, last_result TEXT, error TEXT,
                     result_json TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
                 );
                 INSERT INTO tasks VALUES ('legacy-task', 'pull', 'completed', NULL, 'legacy-operation', 1, 1, NULL, 'completed', NULL, NULL, '{now}', '{now}');"
            ),
        )
        .expect("legacy schema");
    drop(connection);

    let database = Database::open(&path).expect("forward migrate");
    let task = database
        .find_task("legacy-task")
        .expect("find legacy task")
        .expect("legacy task");
    assert_eq!(task.conflict_mode, "policy");
    database
        .with_connection(|connection| {
            let mut statement = connection
                .prepare("PRAGMA table_info(tasks)")
                .map_err(|error| xingshu_core::types::VcsError::Database(error.to_string()))?;
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(|error| xingshu_core::types::VcsError::Database(error.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| xingshu_core::types::VcsError::Database(error.to_string()))?;
            assert!(columns.iter().any(|column| column == "conflict_mode"));
            Ok(())
        })
        .expect("inspect migrated schema");
}

#[test]
fn size_rolling_log_keeps_a_bounded_number_of_files() {
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("xingshu.jsonl");
    let mut writer = logging::SizeRollingFile::new(&path, 20, 2).expect("log writer");
    for _ in 0..8 {
        writer
            .write_all(b"01234567890123456789\n")
            .expect("write log");
    }
    writer.flush().expect("flush log");
    let files = std::fs::read_dir(temp.path())
        .expect("log directory")
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("xingshu.jsonl")
        })
        .count();
    assert!(files <= 3);
    assert!(temp.path().join("xingshu.jsonl.1").is_file());
}
