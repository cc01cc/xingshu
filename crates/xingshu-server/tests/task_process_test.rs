use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use tempfile::TempDir;
use xingshu_core::Database;

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
