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
    let binary = env!("CARGO_BIN_EXE_xs");
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

fn create_fixture_repo(path: &Path) {
    fs::create_dir_all(path).expect("repo dir");
    git(path, &["init", "-q", "-b", "main"]);
    git(path, &["config", "user.email", "test@example.invalid"]);
    git(path, &["config", "user.name", "Xingshu Test"]);
    fs::write(path.join("README.md"), "fixture\n").expect("fixture");
    git(path, &["add", "README.md"]);
    git(path, &["commit", "-q", "-m", "fixture"]);
}

fn output_has_repo_name(output: &str, name: &str) -> bool {
    output.lines().any(|line| {
        line.split_whitespace().any(|column| {
            column
                .trim_end_matches(':')
                .rsplit('/')
                .next()
                .is_some_and(|repo_name| repo_name == name)
        })
    })
}

#[test]
fn cli_dry_run_root_filter_stats_and_skip_alias() {
    let temp = TempDir::new().expect("temp");
    let root_a = temp.path().join("root-a");
    let root_b = temp.path().join("root-b");
    let repo_a = root_a.join("local").join("a");
    let repo_b = root_b.join("local").join("b");
    create_fixture_repo(&repo_a);
    create_fixture_repo(&repo_b);
    let db = temp.path().join("xingshu-test.db");
    let binary = env!("CARGO_BIN_EXE_xs");
    let db_arg = db.to_str().expect("db");

    for args in [
        vec!["roots", "add", root_a.to_str().expect("root-a")],
        vec!["roots", "add", root_b.to_str().expect("root-b")],
        vec!["scan"],
        vec!["tag", "local/a", "demo"],
    ] {
        let mut command = vec!["--db", db_arg];
        command.extend(args);
        let output = Command::new(binary).args(&command).output().expect("cli");
        assert!(
            output.status.success(),
            "{:?}: {}",
            command,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // ISS-10: stats --by root/tag
    let output = Command::new(binary)
        .args(["--db", db_arg, "stats", "--by", "root"])
        .output()
        .expect("stats root");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("by root:"), "{stdout}");
    assert!(
        stdout.contains("root-a") && stdout.contains("root-b"),
        "{stdout}"
    );
    let output = Command::new(binary)
        .args(["--db", db_arg, "stats", "--by", "tag"])
        .output()
        .expect("stats tag");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("by tag:") && stdout.contains("demo"),
        "{stdout}"
    );
    assert!(stdout.contains("multiple tags"), "{stdout}");

    // ISS-04: dry-run is zero-write; dirty repo is reported as a conflict.
    fs::write(repo_a.join("dirty.txt"), "uncommitted\n").expect("dirty");
    let output = Command::new(binary)
        .args(["--db", db_arg, "pull", "--dry-run"])
        .output()
        .expect("dry-run");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("local/a: conflict(dirty)"), "{stdout}");
    assert!(stdout.contains("local/b: clean"), "{stdout}");
    let database = xingshu_core::Database::open(&db).expect("open db");
    for repo in database.list_repos().expect("repos") {
        assert!(
            repo.last_pull_status.is_none(),
            "dry-run must not update status: {repo:?}"
        );
        assert!(
            database
                .list_fetch_logs(repo.id.unwrap_or_default(), 10)
                .expect("logs")
                .is_empty(),
            "dry-run must not write fetch_log"
        );
    }

    // ISS-09: pull --root limits the batch to one root.
    let roots = database.list_roots().expect("roots");
    let root_b_id = roots
        .iter()
        .find(|root| root.path == root_b)
        .and_then(|root| root.id)
        .expect("root-b id");
    let output = Command::new(binary)
        .args([
            "--db",
            db_arg,
            "pull",
            "--dry-run",
            "--root",
            &root_b_id.to_string(),
        ])
        .output()
        .expect("dry-run root");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("local/b:"), "{stdout}");
    assert!(!stdout.contains("local/a:"), "{stdout}");

    // ISS-06: canonical value is `skip`; legacy `abort` is accepted as alias.
    for value in ["skip", "abort"] {
        let output = Command::new(binary)
            .args([
                "--db",
                db_arg,
                "policy",
                "set",
                "local/a",
                "--unattended",
                value,
            ])
            .output()
            .expect("policy set");
        assert!(
            output.status.success(),
            "{value}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = Command::new(binary)
        .args([
            "--db",
            db_arg,
            "policy",
            "set",
            "local/a",
            "--unattended",
            "bogus",
        ])
        .output()
        .expect("policy set bogus");
    assert!(!output.status.success());
}

#[test]
fn cli_pull_conflict_fallback_backs_up_and_audits() {
    let temp = TempDir::new().expect("temp");
    let root = temp.path().join("root-a");
    let repo_path = root.join("local").join("x");
    let remote = temp.path().join("remote").join("x.git");
    fs::create_dir_all(&remote).expect("remote dir");
    git(&remote, &["init", "-q", "--bare"]);
    create_fixture_repo(&repo_path);
    git(
        &repo_path,
        &["remote", "add", "origin", remote.to_str().expect("remote")],
    );
    git(&repo_path, &["push", "-q", "-u", "origin", "main"]);
    fs::write(repo_path.join("README.md"), "dirty\n").expect("dirty");

    let db = temp.path().join("xingshu-test.db");
    let binary = env!("CARGO_BIN_EXE_xs");
    let db_arg = db.to_str().expect("db");
    for args in [
        vec!["roots", "add", root.to_str().expect("root")],
        vec!["scan"],
        vec!["policy", "set", "local/x", "--unattended", "backup"],
    ] {
        let mut command = vec!["--db", db_arg];
        command.extend(args);
        let output = Command::new(binary).args(&command).output().expect("cli");
        assert!(
            output.status.success(),
            "{:?}: {}",
            command,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Non-TTY pull (stdin null) must fall back to the unattended policy and
    // apply `backup` without hanging (PLAN-297 ISS-01/ISS-07).
    let output = Command::new(binary)
        .stdin(std::process::Stdio::null())
        .args(["--db", db_arg, "pull"])
        .output()
        .expect("pull");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout
            .lines()
            .any(|line| output_has_repo_name(line, "x") && line.contains(": ok")),
        "{stdout}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unattended"), "{stderr}");

    let has_backup = fs::read_dir(root.join("local"))
        .expect("local dir")
        .filter_map(Result::ok)
        .any(|entry| entry.file_name().to_string_lossy().starts_with("x.bak."));
    assert!(has_backup, "backup directory missing");
    assert!(
        repo_path.join(".git").is_dir(),
        "repository was not re-cloned"
    );
    assert!(
        repo_path.join("README.md").read_link().is_err(),
        "sanity: regular file expected"
    );

    let database = xingshu_core::Database::open(&db).expect("open db");
    let repo = database
        .list_repos()
        .expect("repos")
        .into_iter()
        .find(|repo| repo.name == "x")
        .expect("repo x");
    assert_eq!(repo.last_pull_status.as_deref(), Some("ok"));
    let logs = database
        .list_fetch_logs(repo.id.unwrap_or_default(), 10)
        .expect("logs");
    assert!(
        logs.iter().any(|log| log.result == "ok"),
        "missing fetch_log: {logs:?}"
    );
}

#[test]
fn cli_status_reports_local_state_and_remote_drift() {
    let temp = TempDir::new().expect("temp");
    let root = temp.path().join("root-a");
    let repo_path = root.join("local").join("s");
    let remote = temp.path().join("remote").join("s.git");
    fs::create_dir_all(&remote).expect("remote dir");
    git(&remote, &["init", "-q", "--bare"]);
    git(&remote, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    create_fixture_repo(&repo_path);
    git(
        &repo_path,
        &["remote", "add", "origin", remote.to_str().expect("remote")],
    );
    git(&repo_path, &["push", "-q", "-u", "origin", "main"]);

    let db = temp.path().join("xingshu-test.db");
    let binary = env!("CARGO_BIN_EXE_xs");
    let db_arg = db.to_str().expect("db");
    for args in [
        vec!["roots", "add", root.to_str().expect("root")],
        vec!["scan"],
    ] {
        let mut command = vec!["--db", db_arg];
        command.extend(args);
        let output = Command::new(binary).args(&command).output().expect("cli");
        assert!(output.status.success(), "{command:?}");
    }

    let output = Command::new(binary)
        .args(["--db", db_arg, "status"])
        .output()
        .expect("status");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output_has_repo_name(&stdout, "s"), "{stdout}");

    let output = Command::new(binary)
        .args(["--db", db_arg, "status", "--remote"])
        .output()
        .expect("status remote");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("up-to-date"), "{stdout}");

    // Move the remote from a second clone; the original has not fetched yet.
    let other = temp.path().join("other");
    git(
        temp.path(),
        &[
            "clone",
            "-q",
            remote.to_str().expect("remote"),
            other.to_str().expect("other"),
        ],
    );
    git(&other, &["config", "user.email", "test@example.invalid"]);
    git(&other, &["config", "user.name", "Xingshu Test"]);
    fs::write(other.join("README.md"), "remote moved\n").expect("remote change");
    git(&other, &["add", "README.md"]);
    git(&other, &["commit", "-q", "-m", "remote move"]);
    git(&other, &["push", "-q", "origin", "main"]);

    let output = Command::new(binary)
        .args(["--db", db_arg, "status", "--remote"])
        .output()
        .expect("status remote moved");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("changed"), "{stdout}");
}

#[test]
fn db_path_uses_cli_flag_before_env_and_does_not_create_database() {
    let temp = TempDir::new().expect("temp");
    let cwd = temp.path().join("cwd");
    fs::create_dir_all(&cwd).expect("cwd");
    let cli_db = Path::new("missing/cli.db");
    let env_db = temp.path().join("env.db");
    let no_git_path = temp.path().join("no-git");
    fs::create_dir_all(&no_git_path).expect("no-git path");
    let expected = cwd.join(cli_db);
    let binary = env!("CARGO_BIN_EXE_xs");

    let output = Command::new(binary)
        .current_dir(&cwd)
        .args(["db", "path", "--db", cli_db.to_str().expect("cli db")])
        .env("XINGSHU_DB", &env_db)
        .env("XINGSHU_ENV", "dev")
        .env("PATH", &no_git_path)
        .env_remove("XINGSHU_LOG_FILE")
        .output()
        .expect("db path");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("JSON response");
    assert_eq!(response["environment"], "dev");
    assert_eq!(response["dbPath"], expected.to_string_lossy().as_ref());
    assert!(!expected.parent().expect("parent").exists());
    assert!(!env_db.exists());
}

#[test]
fn db_path_uses_env_db_before_default_and_does_not_open_database() {
    let temp = TempDir::new().expect("temp");
    let cwd = temp.path().join("cwd");
    fs::create_dir_all(&cwd).expect("cwd");
    let env_db = Path::new("custom/env.db");
    let expected = cwd.join(env_db);
    let binary = env!("CARGO_BIN_EXE_xs");

    let output = Command::new(binary)
        .current_dir(&cwd)
        .args(["db", "path"])
        .env("XINGSHU_DB", env_db)
        .env("XINGSHU_ENV", "prod")
        .env_remove("XINGSHU_LOG_FILE")
        .output()
        .expect("db path");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("JSON response");
    assert_eq!(response["environment"], "prod");
    assert_eq!(response["dbPath"], expected.to_string_lossy().as_ref());
    assert!(!expected.parent().expect("parent").exists());
}

#[test]
fn db_path_rejects_invalid_env_and_prod_staging_path() {
    let temp = TempDir::new().expect("temp");
    let cwd = temp.path().join("cwd");
    fs::create_dir_all(&cwd).expect("cwd");
    let binary = env!("CARGO_BIN_EXE_xs");

    let invalid = Command::new(binary)
        .current_dir(&cwd)
        .args(["db", "path"])
        .env("XINGSHU_ENV", "production")
        .env_remove("XINGSHU_LOG_FILE")
        .output()
        .expect("invalid environment");
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("invalid XINGSHU_ENV"));

    let staging_db = Path::new(".staging/rejected.db");
    let guarded = Command::new(binary)
        .current_dir(&cwd)
        .args([
            "--db",
            staging_db.to_str().expect("staging db"),
            "db",
            "path",
        ])
        .env("XINGSHU_ENV", "prod")
        .env_remove("XINGSHU_LOG_FILE")
        .output()
        .expect("prod staging guard");
    assert!(!guarded.status.success());
    assert!(String::from_utf8_lossy(&guarded.stderr).contains("refusing to open staging database"));
    assert!(!cwd.join(staging_db).exists());
}

#[test]
fn git_dependent_command_reports_how_to_install_git() {
    let temp = TempDir::new().expect("temp");
    let root = temp.path().join("root");
    create_fixture_repo(&root.join("local").join("demo"));
    let no_git_path = temp.path().join("no-git");
    fs::create_dir_all(&no_git_path).expect("no-git path");
    let db = temp.path().join("data/xingshu-test.db");
    let binary = env!("CARGO_BIN_EXE_xs");

    let setup = Command::new(binary)
        .current_dir(temp.path())
        .args([
            "--db",
            db.to_str().expect("db"),
            "roots",
            "add",
            root.to_str().expect("root"),
        ])
        .env_remove("XINGSHU_LOG_FILE")
        .output()
        .expect("register root");
    assert!(
        setup.status.success(),
        "{}",
        String::from_utf8_lossy(&setup.stderr)
    );

    for git_command in [
        vec!["scan"],
        vec!["search", "needle"],
        vec!["files", "README.md"],
    ] {
        let output = Command::new(binary)
            .current_dir(temp.path())
            .args(["--db", db.to_str().expect("db")])
            .args(&git_command)
            .env("PATH", &no_git_path)
            .env_remove("XINGSHU_LOG_FILE")
            .output()
            .unwrap_or_else(|error| panic!("{git_command:?} without Git: {error}"));
        assert!(
            !output.status.success(),
            "{git_command:?} unexpectedly succeeded"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Git executable not found on PATH") && stderr.contains("install Git"),
            "{git_command:?}: {stderr}"
        );
    }
    assert!(
        db.exists(),
        "preflight runs after the existing DB was set up"
    );
}
