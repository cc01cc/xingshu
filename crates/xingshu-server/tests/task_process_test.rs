use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use chrono::Utc;
use tempfile::TempDir;
use xingshu_core::Database;
use xingshu_core::scanner::scan_roots;
use xingshu_core::types::{Root, ScanOptions};

fn http_request(port: u16, method: &str, path: &str, body: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect server");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("read timeout");
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).expect("write request");
    let mut response = Vec::new();
    stream.read_to_end(&mut response).expect("read response");
    String::from_utf8(response)
        .expect("HTTP response")
        .replace("\r\n", "\n")
}

fn response_body(response: &str) -> &str {
    response
        .split_once("\n\n")
        .map(|(_, body)| body)
        .expect("HTTP body")
}

fn start_server(db: &std::path::Path, port: u16) -> Child {
    Command::new(env!("CARGO_BIN_EXE_xingshu-server"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("XINGSHU_DB", db)
        .env("XINGSHU_HOST", "127.0.0.1")
        .env("XINGSHU_PORT", port.to_string())
        .env("XINGSHU_LOG_LEVEL", "error")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start server")
}

fn stop_server(mut child: Child) -> String {
    let _ = child.kill();
    let output = child.wait_with_output().expect("wait server");
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn git(path: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn prepare_dirty_repo(temp: &TempDir, database: &Database) -> (i64, std::path::PathBuf) {
    let root_path = temp.path().join("root");
    let repo_path = root_path.join("network/demo");
    fs::create_dir_all(&repo_path).expect("repo");
    git(&repo_path, &["init", "-q", "-b", "main"]);
    git(
        &repo_path,
        &["config", "user.email", "test@example.invalid"],
    );
    git(&repo_path, &["config", "user.name", "Xingshu Test"]);
    fs::write(repo_path.join("README.md"), "fixture\n").expect("fixture");
    git(&repo_path, &["add", "README.md"]);
    git(&repo_path, &["commit", "-q", "-m", "fixture"]);
    fs::write(repo_path.join("local.txt"), "dirty\n").expect("dirty fixture");

    let now = Utc::now();
    let root = Root {
        id: None,
        name: "root".to_owned(),
        path: root_path.clone(),
        disk_label: None,
        mount_point: None,
        priority: 0,
        created_at: now,
        updated_at: now,
    };
    let root_id = database.upsert_root(&root).expect("root");
    let mut registered = root;
    registered.id = Some(root_id);
    scan_roots(database, &[registered], &ScanOptions::default()).expect("scan");
    let repo_id = database
        .find_repo("network/demo")
        .expect("repo lookup")
        .expect("repo")
        .id
        .expect("repo id");
    (repo_id, repo_path)
}

#[test]
fn compiled_server_runs_task_http_and_sse_flow() {
    let temp = TempDir::new().expect("temp dir");
    let database_path = temp.path().join("index.db");
    Database::open(&database_path).expect("database");
    let probe = TcpListener::bind(("127.0.0.1", 0)).expect("reserve port");
    let port = probe.local_addr().expect("port").port();
    drop(probe);

    let child = start_server(&database_path, port);
    let mut ready = false;
    for _ in 0..100 {
        if let Ok(response) = TcpStream::connect(("127.0.0.1", port)) {
            drop(response);
            let health = http_request(port, "GET", "/health", "");
            if health.contains("200 OK") && health.contains("\"status\":\"ok\"") {
                ready = true;
                break;
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    if !ready {
        let stderr = stop_server(child);
        panic!("server did not become ready: {stderr}");
    }

    let create = http_request(port, "POST", "/api/v1/tasks", r#"{"type":"scan"}"#);
    assert!(create.contains("202 Accepted"), "{create}");
    let created: serde_json::Value =
        serde_json::from_str(response_body(&create)).expect("create json");
    let task_id = created["taskId"].as_str().expect("task id").to_owned();

    let mut final_task = None;
    for _ in 0..100 {
        let response = http_request(port, "GET", &format!("/api/v1/tasks/{task_id}"), "");
        let value: serde_json::Value =
            serde_json::from_str(response_body(&response)).expect("task json");
        if matches!(value["status"].as_str(), Some("completed" | "failed")) {
            final_task = Some(value);
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let final_task = final_task.expect("terminal task");
    assert_eq!(final_task["taskId"], task_id);
    assert_eq!(final_task["type"], "scan");
    assert_eq!(final_task["status"], "completed");

    let stream = http_request(port, "GET", &format!("/api/v1/tasks/{task_id}/stream"), "");
    assert!(stream.contains("event: snapshot"), "{stream}");
    assert!(stream.contains("\"taskId\""), "{stream}");
    assert!(stream.contains("\"status\":\"completed\""), "{stream}");

    let stderr = stop_server(child);
    assert!(stderr.is_empty(), "server stderr: {stderr}");
}

#[test]
fn compiled_server_resolves_task_conflict_through_http() {
    let temp = TempDir::new().expect("temp dir");
    let database_path = temp.path().join("index.db");
    let database = Database::open(&database_path).expect("database");
    let (repo_id, repo_path) = prepare_dirty_repo(&temp, &database);
    let probe = TcpListener::bind(("127.0.0.1", 0)).expect("reserve port");
    let port = probe.local_addr().expect("port").port();
    drop(probe);

    let child = start_server(&database_path, port);
    let mut ready = false;
    for _ in 0..100 {
        if let Ok(response) = TcpStream::connect(("127.0.0.1", port)) {
            drop(response);
            let health = http_request(port, "GET", "/health", "");
            if health.contains("200 OK") && health.contains("\"status\":\"ok\"") {
                ready = true;
                break;
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    if !ready {
        let stderr = stop_server(child);
        panic!("server did not become ready: {stderr}");
    }

    let create = http_request(
        port,
        "POST",
        "/api/v1/tasks",
        r#"{"type":"pull","conflictMode":"ask"}"#,
    );
    assert!(create.contains("202 Accepted"), "{create}");
    let created: serde_json::Value =
        serde_json::from_str(response_body(&create)).expect("create json");
    let task_id = created["taskId"].as_str().expect("task id").to_owned();

    let mut waiting = false;
    for _ in 0..100 {
        let response = http_request(port, "GET", &format!("/api/v1/tasks/{task_id}"), "");
        let value: serde_json::Value =
            serde_json::from_str(response_body(&response)).expect("task json");
        if value["status"] == "waiting_for_decision" {
            waiting = true;
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(waiting, "task did not enter waiting_for_decision");

    let conflicts = http_request(
        port,
        "GET",
        &format!("/api/v1/tasks/{task_id}/conflicts"),
        "",
    );
    assert!(conflicts.contains("200 OK"), "{conflicts}");
    let conflicts: serde_json::Value =
        serde_json::from_str(response_body(&conflicts)).expect("conflicts json");
    assert_eq!(conflicts.as_array().map(Vec::len), Some(1));
    assert_eq!(conflicts[0]["repoId"], repo_id);
    assert_eq!(conflicts[0]["status"], "waiting_decision");

    let decision = http_request(
        port,
        "POST",
        &format!("/api/v1/tasks/{task_id}/repos/{repo_id}/decision"),
        r#"{"action":"abort"}"#,
    );
    assert!(decision.contains("202 Accepted"), "{decision}");

    let mut finished = false;
    for _ in 0..100 {
        let response = http_request(port, "GET", &format!("/api/v1/tasks/{task_id}"), "");
        let value: serde_json::Value =
            serde_json::from_str(response_body(&response)).expect("task json");
        if value["status"] == "completed" {
            finished = true;
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(finished, "task did not complete after decision");
    let task_repo = Database::open(&database_path)
        .expect("open database")
        .find_task_repo(&task_id, repo_id)
        .expect("task repo lookup")
        .expect("task repo");
    assert_eq!(task_repo.status, "aborted");
    assert_eq!(task_repo.result.as_deref(), Some("aborted"));
    assert!(repo_path.join("local.txt").is_file());
    let conflicts = http_request(
        port,
        "GET",
        &format!("/api/v1/tasks/{task_id}/conflicts"),
        "",
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(response_body(&conflicts))
            .expect("final conflicts json"),
        serde_json::json!([])
    );

    let stderr = stop_server(child);
    assert!(stderr.is_empty(), "server stderr: {stderr}");
}
