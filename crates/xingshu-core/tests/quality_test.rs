use std::io::Write;

use chrono::Utc;
use tempfile::TempDir;
use xingshu_core::types::TaskRecord;
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
            assert_eq!(version, 3);
            for table in [
                "roots",
                "repos",
                "tags",
                "repo_tags",
                "policies",
                "fetch_log",
                "tasks",
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
