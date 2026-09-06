use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

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

#[test]
fn cli_runs_real_process_index_tag_policy_and_filter_flow() {
    let temp = TempDir::new().expect("temp");
    let root = temp.path().join("root").join("local").join("demo");
    fs::create_dir_all(&root).expect("root");
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.email", "test@example.invalid"]);
    git(&root, &["config", "user.name", "Xingshu Test"]);
    fs::write(root.join("README.md"), "cli fixture\n").expect("fixture");
    git(&root, &["add", "README.md"]);
    git(&root, &["commit", "-q", "-m", "fixture"]);
    let db = temp.path().join("xingshu-test.db");
    let binary = env!("CARGO_BIN_EXE_xingshu");
    for args in [
        vec![
            "--db",
            db.to_str().expect("db"),
            "roots",
            "add",
            temp.path().join("root").to_str().expect("path"),
        ],
        vec!["--db", db.to_str().expect("db"), "scan"],
        vec!["--db", db.to_str().expect("db"), "tag", "local/demo", "cli"],
    ] {
        let output = Command::new(binary).args(args).output().expect("xingshu");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = Command::new(binary)
        .args(["--db", db.to_str().expect("db"), "list", "--tag", "cli"])
        .output()
        .expect("list");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("demo"));
    let output = Command::new(binary)
        .args([
            "--db",
            db.to_str().expect("db"),
            "policy",
            "set",
            "local/demo",
            "--strategy",
            "no-update",
        ])
        .output()
        .expect("policy");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let log_file = temp.path().join("logs/xingshu.jsonl");
    let output = Command::new(binary)
        .env("XINGSHU_LOG_FILE", &log_file)
        .env("XINGSHU_LOG_MAX_BYTES", "256")
        .env("XINGSHU_LOG_MAX_FILES", "2")
        .args(["--db", db.to_str().expect("db"), "scan"])
        .output()
        .expect("logged scan");
    assert!(output.status.success());
    let log = std::fs::read_to_string(&log_file).expect("log file");
    assert!(log.contains("scan requested"));

    let pull_log = temp.path().join("logs/pull.jsonl");
    let output = Command::new(binary)
        .env("XINGSHU_LOG_FILE", &pull_log)
        .args(["--db", db.to_str().expect("db"), "pull", "--unattended"])
        .output()
        .expect("logged pull");
    assert!(output.status.success());
    let pull_log = std::fs::read_to_string(pull_log).expect("pull log");
    assert!(pull_log.contains("operation_id"));
    assert!(pull_log.contains("duration_ms"));
}
