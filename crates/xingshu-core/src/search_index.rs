use crate::types::VcsError;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone)]
pub struct SearchRepo {
    pub repo: crate::types::RepoRecord,
    pub root: crate::types::Root,
}

#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    pub pattern: String,
    pub fixed_strings: bool,
    pub ignore_case: bool,
    pub word_regexp: bool,
    pub context: usize,
    pub files_with_matches: bool,
    pub count: bool,
    pub max_count: Option<usize>,
    pub max_results: Option<usize>,
    pub types: Vec<String>,
    pub globs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchOutput {
    pub summary: SearchSummary,
    pub results: Vec<SearchResult>,
    pub errors: Vec<SearchError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SearchSummary {
    pub matches: u64,
    pub files: u64,
    pub repos: u64,
    pub source: String,
    pub complete: bool,
    pub truncated: bool,
    pub max_results: Option<usize>,
    #[serde(skip)]
    pub skipped_files: u64,
    #[serde(skip)]
    pub files_with_matches: bool,
    #[serde(skip)]
    pub count_mode: bool,
    #[serde(skip)]
    pub matched_lines: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    #[serde(skip)]
    pub repo_id: i64,
    #[serde(skip)]
    pub repo_name: String,
    pub repo: String,
    pub root_id: i64,
    pub path: String,
    pub revision: Option<String>,
    pub line: usize,
    pub column: usize,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
    pub source: String,
    pub base_head: Option<String>,
    pub file_state: String,
    pub truncated: bool,
    pub context: Option<SearchContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextRow {
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchContext {
    pub before: Vec<ContextRow>,
    pub after: Vec<ContextRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FilesOutput {
    pub summary: FilesSummary,
    pub results: Vec<FilePathResult>,
    pub errors: Vec<SearchError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_id: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct FilesOptions {
    pub pattern: Option<String>,
    pub types: Vec<String>,
    pub globs: Vec<String>,
    pub max_results: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FilesSummary {
    pub files: u64,
    pub repos: u64,
    pub source: String,
    pub complete: bool,
    pub truncated: bool,
    pub max_results: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePathResult {
    #[serde(skip)]
    pub repo_id: i64,
    #[serde(skip)]
    pub repo_name: String,
    pub repo: String,
    pub root_id: i64,
    pub path: String,
    pub revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub repo: String,
    pub root_id: i64,
    pub current_head: Option<String>,
    pub indexed_commit: Option<String>,
    pub state: String,
    pub text_files: u64,
    pub text_bytes: u64,
    pub index_bytes: u64,
    pub filter_version: i64,
    pub updated_at: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IndexUpdateResult {
    #[serde(skip)]
    pub repo_id: i64,
    pub repo: String,
    pub root_id: i64,
    pub outcome: String,
    pub indexed_commit: Option<String>,
    pub text_files: u64,
    pub text_bytes: u64,
    pub error: Option<String>,
    pub detail: Option<String>,
    pub errors: Vec<SearchError>,
}

#[derive(Debug, Clone, Default)]
pub struct ReadOptions {
    pub path: String,
    pub revision: Option<String>,
    pub start_line: Option<usize>,
    pub max_lines: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadOutput {
    #[serde(skip)]
    pub repo_id: i64,
    #[serde(skip)]
    pub repo_name: String,
    pub repo: String,
    pub root_id: i64,
    pub revision: Option<String>,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
    pub truncated: bool,
    pub source: String,
}

const FILTER_VERSION: i64 = 1;
const MAX_BODY: usize = 5 * 1024 * 1024;

pub struct IndexService {
    sidecar: PathBuf,
}

impl IndexService {
    pub fn open(
        metadata_db_path: impl AsRef<Path>,
        environment: impl Into<String>,
    ) -> Result<Self, VcsError> {
        let db = metadata_db_path.as_ref();
        let absolute = if db.exists() {
            std::fs::canonicalize(db)
        } else {
            let parent = std::fs::canonicalize(db.parent().unwrap_or_else(|| Path::new(".")))
                .map_err(fs_err)?;
            Ok(parent.join(db.file_name().unwrap_or_default()))
        }
        .map_err(fs_err)?;
        let mut hasher = DefaultHasher::new();
        absolute.to_string_lossy().hash(&mut hasher);
        environment.into().hash(&mut hasher);
        Ok(Self {
            sidecar: absolute
                .with_file_name(format!(".xs-search-v2-{:016x}.sqlite", hasher.finish())),
        })
    }

    fn read_db(&self) -> Result<Option<Connection>, VcsError> {
        if !self.sidecar.exists() {
            return Ok(None);
        }
        Connection::open_with_flags(&self.sidecar, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map(Some)
            .map_err(db_err)
    }

    fn write_db(&self) -> Result<Connection, VcsError> {
        let connection = Connection::open(&self.sidecar).map_err(db_err)?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 CREATE TABLE IF NOT EXISTS repos(
                   repo_id INTEGER PRIMARY KEY, root TEXT NOT NULL, head TEXT NOT NULL,
                   filter_version INTEGER NOT NULL, indexed_at TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS files(
                   repo_id INTEGER NOT NULL, path TEXT NOT NULL, oid TEXT NOT NULL,
                   body TEXT, body_bytes INTEGER NOT NULL, PRIMARY KEY(repo_id,path)
                 );
                 CREATE VIRTUAL TABLE IF NOT EXISTS body_fts USING fts5(
                   body, content='files', content_rowid='rowid', tokenize='trigram', detail=none
                 );",
            )
            .map_err(db_err)?;
        Ok(connection)
    }

    pub fn status(&self, repos: &[SearchRepo]) -> Result<Vec<IndexStatus>, VcsError> {
        let db = self.read_db()?;
        repos
            .iter()
            .map(|repo| {
                let id = repo.repo.id.unwrap_or_default();
                let root = repo_path(repo);
                let row: Option<(String, i64)> = db
                    .as_ref()
                    .map(|db| {
                        db.query_row(
                            "SELECT head,filter_version FROM repos WHERE repo_id=?1 AND root=?2",
                            params![id, root.to_string_lossy()],
                            |row| Ok((row.get(0)?, row.get(1)?)),
                        )
                        .optional()
                        .map_err(db_err)
                    })
                    .transpose()?
                    .flatten();
                let (indexed_commit, filter_version) = row
                    .map(|(head, version)| (Some(head), version))
                    .unwrap_or((None, 0));
                let (text_files, text_bytes) = if let Some(db) = db.as_ref() {
                    db.query_row(
                        "SELECT count(body),coalesce(sum(body_bytes),0) FROM files \
                         WHERE repo_id=?1 AND body IS NOT NULL",
                        [id],
                        |row| Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?)),
                    )
                    .map_err(db_err)?
                } else {
                    (0, 0)
                };
                let current_head = current_head(&root).ok();
                let state = if indexed_commit.is_none() {
                    "missing"
                } else if indexed_commit != current_head || filter_version != FILTER_VERSION {
                    "stale"
                } else {
                    "ready"
                };
                Ok(IndexStatus {
                    repo: repo_name(repo),
                    root_id: repo.repo.root_id,
                    current_head,
                    indexed_commit,
                    state: state.to_owned(),
                    text_files,
                    text_bytes,
                    index_bytes: if self.sidecar.exists() {
                        std::fs::metadata(&self.sidecar)
                            .map(|meta| meta.len())
                            .unwrap_or(0)
                    } else {
                        0
                    },
                    filter_version,
                    updated_at: None,
                    error: None,
                })
            })
            .collect()
    }

    pub fn update(
        &mut self,
        repo: &SearchRepo,
        rebuild: bool,
    ) -> Result<IndexUpdateResult, VcsError> {
        use std::collections::{HashMap, HashSet};

        let id = repo
            .repo
            .id
            .ok_or_else(|| VcsError::Database("index requires repo id".into()))?;
        let root = repo_path(repo);
        let head = current_head(&root)?;
        let mut db = self.write_db()?;
        let previous: Option<(String, i64)> = db
            .query_row(
                "SELECT head,filter_version FROM repos WHERE repo_id=?1 AND root=?2",
                params![id, root.to_string_lossy()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(db_err)?;
        let base_available = previous.as_ref().is_some_and(|(commit, _)| {
            run_git(&root, &["cat-file", "-e", &format!("{commit}^{{tree}}")]).is_ok()
        });
        let full_rebuild = rebuild
            || previous
                .as_ref()
                .is_none_or(|(_, version)| *version != FILTER_VERSION)
            || !base_available;
        let tree = git_tree(&root)?;
        let current_paths: HashSet<String> = tree.iter().map(|(path, _, _)| path.clone()).collect();
        let transaction = db.transaction().map_err(db_err)?;
        let existing: HashMap<String, String> = {
            let mut statement = transaction
                .prepare("SELECT path,oid FROM files WHERE repo_id=?1")
                .map_err(db_err)?;
            statement
                .query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(db_err)?
                .collect::<Result<_, _>>()
                .map_err(db_err)?
        };
        if full_rebuild {
            transaction
                .execute(
                    "INSERT INTO body_fts(body_fts,rowid,body) \
                     SELECT 'delete',rowid,body FROM files WHERE repo_id=?1 AND body IS NOT NULL",
                    [id],
                )
                .map_err(db_err)?;
            transaction
                .execute("DELETE FROM files WHERE repo_id=?1", [id])
                .map_err(db_err)?;
        } else {
            for path in existing
                .keys()
                .filter(|path| !current_paths.contains(*path))
            {
                transaction
                    .execute(
                        "INSERT INTO body_fts(body_fts,rowid,body) \
                         SELECT 'delete',rowid,body FROM files \
                         WHERE repo_id=?1 AND path=?2 AND body IS NOT NULL",
                        params![id, path],
                    )
                    .map_err(db_err)?;
                transaction
                    .execute(
                        "DELETE FROM files WHERE repo_id=?1 AND path=?2",
                        params![id, path],
                    )
                    .map_err(db_err)?;
            }
        }

        let mut cat_file: Option<CatFileBatch> = None;
        for (path, oid, object_size) in tree {
            if !full_rebuild && existing.get(&path) == Some(&oid) {
                continue;
            }
            if existing.contains_key(&path) && !full_rebuild {
                transaction
                    .execute(
                        "INSERT INTO body_fts(body_fts,rowid,body) \
                         SELECT 'delete',rowid,body FROM files \
                         WHERE repo_id=?1 AND path=?2 AND body IS NOT NULL",
                        params![id, path],
                    )
                    .map_err(db_err)?;
                transaction
                    .execute(
                        "DELETE FROM files WHERE repo_id=?1 AND path=?2",
                        params![id, path],
                    )
                    .map_err(db_err)?;
            }
            let is_symlink = oid.starts_with("symlink:");
            if !is_symlink && object_size.is_none() {
                return Err(VcsError::GitCommand(format!(
                    "git object size unavailable for {path} ({oid}); refusing partial index generation"
                )));
            }
            let body = if is_symlink
                || object_size.is_some_and(|size| size > MAX_BODY as u64)
                || !text_path_allowed(&path)
            {
                None
            } else {
                if cat_file.is_none() {
                    cat_file = Some(CatFileBatch::spawn(&root)?);
                }
                cat_file
                    .as_mut()
                    .expect("batch process was initialized")
                    .read_blob(&oid)?
            };
            let body_bytes = body.as_ref().map_or(0, |text| text.len() as u64);
            transaction
                .execute(
                    "INSERT INTO files(repo_id,path,oid,body,body_bytes) \
                     VALUES(?1,?2,?3,?4,?5)",
                    params![id, path, oid, body, body_bytes],
                )
                .map_err(db_err)?;
            if let Some(text) = body {
                let rowid = transaction.last_insert_rowid();
                transaction
                    .execute(
                        "INSERT INTO body_fts(rowid,body) VALUES(?1,?2)",
                        params![rowid, text],
                    )
                    .map_err(db_err)?;
            }
        }
        if let Some(batch) = cat_file {
            batch.finish()?;
        }
        transaction
            .execute(
                "INSERT INTO repos VALUES(?1,?2,?3,?4,datetime('now')) \
                 ON CONFLICT(repo_id) DO UPDATE SET root=excluded.root,head=excluded.head, \
                 filter_version=excluded.filter_version,indexed_at=excluded.indexed_at",
                params![id, root.to_string_lossy(), head, FILTER_VERSION],
            )
            .map_err(db_err)?;
        let (text_files, text_bytes): (u64, u64) = transaction
            .query_row(
                "SELECT count(body),coalesce(sum(body_bytes),0) FROM files \
                 WHERE repo_id=?1 AND body IS NOT NULL",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(db_err)?;
        transaction.commit().map_err(db_err)?;
        Ok(IndexUpdateResult {
            repo_id: id,
            repo: repo_name(repo),
            root_id: repo.repo.root_id,
            outcome: if full_rebuild { "rebuilt" } else { "updated" }.to_owned(),
            indexed_commit: Some(head),
            text_files,
            text_bytes,
            error: None,
            detail: None,
            errors: Vec::new(),
        })
    }

    pub fn purge_repos(&mut self, repo_ids: &[i64]) -> Result<(), VcsError> {
        if !self.sidecar.exists() {
            return Ok(());
        }
        let mut db = self.write_db()?;
        let transaction = db.transaction().map_err(db_err)?;
        for repo_id in repo_ids {
            transaction
                .execute(
                    "INSERT INTO body_fts(body_fts,rowid,body) \
                     SELECT 'delete',rowid,body FROM files WHERE repo_id=?1 AND body IS NOT NULL",
                    [repo_id],
                )
                .map_err(db_err)?;
            transaction
                .execute("DELETE FROM files WHERE repo_id=?1", [repo_id])
                .map_err(db_err)?;
            transaction
                .execute("DELETE FROM repos WHERE repo_id=?1", [repo_id])
                .map_err(db_err)?;
        }
        transaction.commit().map_err(db_err)
    }

    pub(crate) fn indexed_docs_for_literal(
        &self,
        repos: &[SearchRepo],
        pattern: &str,
    ) -> Result<Vec<(SearchRepo, String, Option<String>)>, VcsError> {
        let Some(db) = self.read_db()? else {
            return Err(VcsError::Database("search index missing".into()));
        };
        let mut output = Vec::new();
        for repo in repos {
            let mut statement = db
                .prepare(
                    "SELECT f.path,f.body FROM files f \
                     JOIN body_fts ON body_fts.rowid=f.rowid \
                     WHERE f.repo_id=?1 AND body_fts.body LIKE ?2 ORDER BY f.path",
                )
                .map_err(db_err)?;
            let rows = statement
                .query_map(
                    params![repo.repo.id.unwrap_or_default(), format!("%{pattern}%")],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
                )
                .map_err(db_err)?;
            for row in rows {
                let (path, body) = row.map_err(db_err)?;
                output.push((repo.clone(), path, body));
            }
        }
        Ok(output)
    }

    pub(crate) fn indexed_docs(
        &self,
        repos: &[SearchRepo],
    ) -> Result<Vec<(SearchRepo, String, Option<String>)>, VcsError> {
        let Some(db) = self.read_db()? else {
            return Err(VcsError::Database("search index missing".into()));
        };
        let mut output = Vec::new();
        for repo in repos {
            let mut statement = db
                .prepare("SELECT path,body FROM files WHERE repo_id=?1")
                .map_err(db_err)?;
            for row in statement
                .query_map([repo.repo.id.unwrap_or_default()], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                })
                .map_err(db_err)?
            {
                let (path, body) = row.map_err(db_err)?;
                output.push((repo.clone(), path, body));
            }
        }
        Ok(output)
    }

    pub(crate) fn paths_for_repo(&self, repo: &SearchRepo) -> Result<Vec<String>, VcsError> {
        let Some(db) = self.read_db()? else {
            return Err(VcsError::Database("search index missing".into()));
        };
        let mut statement = db
            .prepare("SELECT path FROM files WHERE repo_id=?1 ORDER BY path")
            .map_err(db_err)?;
        statement
            .query_map([repo.repo.id.unwrap_or_default()], |row| {
                row.get::<_, String>(0)
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)
    }
}

pub(crate) fn repo_path(repo: &SearchRepo) -> PathBuf {
    repo.root.path.join(&repo.repo.rel_path)
}

pub(crate) fn run_git(root: &Path, args: &[&str]) -> Result<Vec<u8>, VcsError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_NO_LAZY_FETCH", "1")
        .output()
        .map_err(|error| VcsError::GitCommand(error.to_string()))?;
    if !output.status.success() {
        return Err(VcsError::GitCommand(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(output.stdout)
}

pub(crate) fn current_head(root: &Path) -> Result<String, VcsError> {
    if let Some(head) = current_head_from_files(root) {
        return Ok(head);
    }
    String::from_utf8(run_git(root, &["rev-parse", "HEAD"])?)
        .map(|value| value.trim().to_owned())
        .map_err(|error| VcsError::GitCommand(error.to_string()))
}

fn git_tree(root: &Path) -> Result<Vec<(String, String, Option<u64>)>, VcsError> {
    let output = run_git(root, &["ls-tree", "-l", "-rz", "--full-tree", "HEAD"])?;
    let mut entries = Vec::new();
    for record in output
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let Some(tab) = record.iter().position(|byte| *byte == b'\t') else {
            continue;
        };
        let metadata: Vec<String> = String::from_utf8_lossy(&record[..tab])
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let mode = metadata.first().map(String::as_str).unwrap_or_default();
        if metadata.get(1).map(String::as_str) != Some("blob") {
            continue;
        }
        let oid = metadata.get(2).cloned().unwrap_or_default();
        let size = metadata.get(3).and_then(|value| value.parse::<u64>().ok());
        let path = String::from_utf8_lossy(&record[tab + 1..]).into_owned();
        entries.push((
            path,
            if mode == "120000" {
                format!("symlink:{oid}")
            } else {
                oid
            },
            size,
        ));
    }
    Ok(entries)
}

struct CatFileBatch {
    child: Option<std::process::Child>,
    stdin: Option<std::process::ChildStdin>,
    stdout: std::io::BufReader<std::process::ChildStdout>,
}

impl CatFileBatch {
    fn spawn(root: &Path) -> Result<Self, VcsError> {
        use std::process::Stdio;

        let mut child = Command::new("git")
            .args(["cat-file", "--batch"])
            .current_dir(root)
            .env("GIT_NO_LAZY_FETCH", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(git_io_err)?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| VcsError::GitCommand("git cat-file stdin unavailable".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| VcsError::GitCommand("git cat-file stdout unavailable".into()))?;
        Ok(Self {
            child: Some(child),
            stdin: Some(stdin),
            stdout: std::io::BufReader::new(stdout),
        })
    }

    fn read_blob(&mut self, oid: &str) -> Result<Option<String>, VcsError> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| VcsError::GitCommand("git cat-file batch is closed".into()))?;
        writeln!(stdin, "{oid}").map_err(git_io_err)?;
        stdin.flush().map_err(git_io_err)?;

        let mut header = String::new();
        self.stdout.read_line(&mut header).map_err(git_io_err)?;
        let mut fields = header.split_whitespace();
        let returned_oid = fields.next().unwrap_or_default();
        let object_type = fields.next().unwrap_or_default();
        let size = fields
            .next()
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| {
                VcsError::GitCommand(format!("unexpected git cat-file response: {header:?}"))
            })?;
        if returned_oid != oid || object_type != "blob" {
            return Err(VcsError::GitCommand(format!(
                "git cat-file returned an unexpected object for {oid}"
            )));
        }

        let mut bytes = Vec::new();
        if size <= MAX_BODY as u64 {
            bytes.reserve(size as usize);
            self.stdout
                .by_ref()
                .take(size)
                .read_to_end(&mut bytes)
                .map_err(git_io_err)?;
            if bytes.len() as u64 != size {
                return Err(VcsError::GitCommand(format!(
                    "truncated git object response for {oid}"
                )));
            }
        } else {
            std::io::copy(&mut self.stdout.by_ref().take(size), &mut std::io::sink())
                .map_err(git_io_err)?;
        }
        let mut terminator = [0_u8; 1];
        self.stdout
            .read_exact(&mut terminator)
            .map_err(git_io_err)?;
        if terminator[0] != b'\n' {
            return Err(VcsError::GitCommand(format!(
                "invalid git cat-file terminator for {oid}"
            )));
        }
        if size > MAX_BODY as u64 || bytes.contains(&0) {
            return Ok(None);
        }
        Ok(String::from_utf8(bytes).ok())
    }

    fn finish(mut self) -> Result<(), VcsError> {
        self.stdin.take();
        let status = self
            .child
            .take()
            .ok_or_else(|| VcsError::GitCommand("git cat-file process is missing".into()))?
            .wait()
            .map_err(git_io_err)?;
        if !status.success() {
            return Err(VcsError::GitCommand("git cat-file --batch failed".into()));
        }
        Ok(())
    }
}

impl Drop for CatFileBatch {
    fn drop(&mut self) {
        self.stdin.take();
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn git_tree_file(root: &Path) -> Option<PathBuf> {
    let dot_git = root.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    if dot_git.is_file() {
        let content = std::fs::read_to_string(dot_git).ok()?;
        let value = content.trim().strip_prefix("gitdir:")?.trim();
        let path = PathBuf::from(value);
        return Some(if path.is_absolute() {
            path
        } else {
            root.join(path)
        });
    }
    if root.join("HEAD").is_file() && root.join("objects").is_dir() {
        return Some(root.to_owned());
    }
    None
}

pub(crate) fn current_head_from_files(root: &Path) -> Option<String> {
    let git_dir = git_tree_file(root)?;
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let reference = head.trim().strip_prefix("ref: ");
    if let Some(reference) = reference {
        if !reference.starts_with("refs/")
            || Path::new(reference)
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return None;
        }
        for refs_dir in [
            git_dir.clone(),
            std::fs::read_to_string(git_dir.join("commondir"))
                .map(|path| git_dir.join(path.trim()))
                .unwrap_or_else(|_| git_dir.clone()),
        ] {
            if let Ok(value) = std::fs::read_to_string(refs_dir.join(reference)) {
                let oid = value.trim();
                if is_object_id(oid) {
                    return Some(oid.to_owned());
                }
            }
            if let Ok(packed) = std::fs::read_to_string(refs_dir.join("packed-refs")) {
                for line in packed.lines() {
                    if line.starts_with('#') || line.starts_with('^') {
                        continue;
                    }
                    let mut fields = line.split_whitespace();
                    let Some(oid) = fields.next() else {
                        continue;
                    };
                    if fields.next() == Some(reference) && is_object_id(oid) {
                        return Some(oid.to_owned());
                    }
                }
            }
        }
        None
    } else {
        let oid = head.trim();
        is_object_id(oid).then(|| oid.to_owned())
    }
}

fn is_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn repo_name(repo: &SearchRepo) -> String {
    format!("{}/{}", repo.repo.org, repo.repo.name)
}

pub(crate) fn text_path_allowed(path: &str) -> bool {
    if path
        .split('/')
        .any(crate::type_extensions::is_generated_component)
    {
        return false;
    }
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    !crate::type_extensions::is_binary_extension(&extension)
}

fn git_io_err(error: std::io::Error) -> VcsError {
    VcsError::GitCommand(error.to_string())
}

fn fs_err(error: std::io::Error) -> VcsError {
    VcsError::Filesystem(error.to_string())
}

fn db_err(error: rusqlite::Error) -> VcsError {
    VcsError::Database(error.to_string())
}

#[cfg(test)]
mod head_resolution_tests {
    use super::{current_head, current_head_from_files};
    use std::{path::Path, process::Command};
    use tempfile::TempDir;

    fn git(path: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .expect("git should start");
        assert!(
            output.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    #[test]
    fn current_head_files_handles_loose_packed_bare_and_linked_worktree_refs() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "user.email", "head@example.invalid"]);
        git(&repo, &["config", "user.name", "Head Test"]);
        std::fs::write(repo.join("file.txt"), "head test\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "initial"]);
        let expected = git(&repo, &["rev-parse", "HEAD"]);
        assert_eq!(
            current_head_from_files(&repo).as_deref(),
            Some(expected.as_str())
        );

        git(&repo, &["pack-refs", "--all", "--prune"]);
        let packed_refs = std::fs::read_to_string(repo.join(".git/packed-refs")).unwrap();
        assert!(
            packed_refs
                .lines()
                .any(|line| line.ends_with("refs/heads/main"))
        );
        let loose_ref = repo.join(".git/refs/heads/main");
        if loose_ref.exists() {
            std::fs::remove_file(&loose_ref).unwrap();
        }
        assert_eq!(
            current_head_from_files(&repo).as_deref(),
            Some(expected.as_str())
        );
        assert_eq!(current_head(&repo).unwrap(), expected);

        let bare = temp.path().join("bare.git");
        let clone = Command::new("git")
            .arg("clone")
            .arg("--bare")
            .arg(&repo)
            .arg(&bare)
            .output()
            .expect("git clone --bare should start");
        assert!(
            clone.status.success(),
            "{}",
            String::from_utf8_lossy(&clone.stderr)
        );
        assert_eq!(
            current_head_from_files(&bare).as_deref(),
            Some(expected.as_str())
        );

        let linked = temp.path().join("linked-worktree");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "--detach",
                linked.to_str().unwrap(),
                "HEAD",
            ],
        );
        assert!(linked.join(".git").is_file());
        assert_eq!(
            current_head_from_files(&linked).as_deref(),
            Some(expected.as_str())
        );
    }
}
