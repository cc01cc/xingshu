use std::fs;
use std::path::Path;
use std::process::Command;

use chrono::Utc;
use tempfile::TempDir;
use xingshu_core::db::Database;
use xingshu_core::policy::{PullConflictAction, ResolvedPolicy};
use xingshu_core::puller::{PullMode, pull_repo};
use xingshu_core::scanner::scan_roots;
use xingshu_core::types::{Root, ScanOptions, VcsError};

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

fn seed_remote(temp: &TempDir) -> (std::path::PathBuf, std::path::PathBuf) {
    let remote = temp.path().join("remote.git");
    Command::new("git")
        .args(["init", "--bare", "-q", remote.to_string_lossy().as_ref()])
        .output()
        .expect("git");
    let seed = temp.path().join("seed");
    fs::create_dir_all(&seed).expect("seed");
    git(&seed, &["init", "-q", "-b", "main"]);
    git(&seed, &["config", "user.email", "test@example.invalid"]);
    git(&seed, &["config", "user.name", "Xingshu Test"]);
    fs::write(seed.join("README.md"), "seed\n").expect("file");
    git(&seed, &["add", "README.md"]);
    git(&seed, &["commit", "-q", "-m", "seed"]);
    git(
        &seed,
        &["remote", "add", "origin", remote.to_string_lossy().as_ref()],
    );
    git(&seed, &["push", "-q", "-u", "origin", "main"]);
    git(&remote, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    (remote, seed)
}

fn scanned_repo(database: &Database, root: &Path, repo_path: &Path) -> xingshu_core::RepoRecord {
    let now = Utc::now();
    let root_record = Root {
        id: None,
        name: "fixture".to_owned(),
        path: root.to_path_buf(),
        disk_label: None,
        mount_point: None,
        priority: 0,
        created_at: now,
        updated_at: now,
    };
    let root_id = database.upsert_root(&root_record).expect("root");
    let mut registered = root_record;
    registered.id = Some(root_id);
    scan_roots(database, &[registered], &ScanOptions::default()).expect("scan");
    let relative = repo_path.strip_prefix(root).expect("relative");
    database
        .find_repo(&relative.to_string_lossy().replace('\\', "/"))
        .expect("lookup")
        .expect("repo")
}

fn policy(action: PullConflictAction) -> ResolvedPolicy {
    ResolvedPolicy {
        pull_strategy: "fetch-only".to_owned(),
        conflict_action: action,
        unattended_action: action,
    }
}

#[test]
fn backup_preserves_dirty_repository_and_reclones_staging_copy() {
    let temp = TempDir::new().expect("temp");
    let (remote, _) = seed_remote(&temp);
    let root = temp.path().join("root");
    let repo_path = root.join("network/demo");
    fs::create_dir_all(repo_path.parent().expect("parent")).expect("parent");
    Command::new("git")
        .args([
            "clone",
            "-q",
            remote.to_string_lossy().as_ref(),
            repo_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("clone");
    fs::write(repo_path.join("local.txt"), "keep me\n").expect("dirty");
    let database = Database::open(temp.path().join("index.db")).expect("db");
    let repo = scanned_repo(&database, &root, &repo_path);
    let outcome = pull_repo(
        &repo_path,
        &repo,
        &policy(PullConflictAction::Backup),
        PullMode::Unattended,
    )
    .expect("backup");
    let backup = outcome.backup_path.expect("backup path");
    assert!(backup.join("local.txt").is_file());
    assert!(repo_path.join(".git").is_dir());
    assert!(!repo_path.join("local.txt").exists());
}

#[test]
fn overwrite_removes_dirty_staging_files() {
    let temp = TempDir::new().expect("temp");
    let (remote, _) = seed_remote(&temp);
    let root = temp.path().join("root");
    let repo_path = root.join("network/demo");
    fs::create_dir_all(repo_path.parent().expect("parent")).expect("parent");
    Command::new("git")
        .args([
            "clone",
            "-q",
            remote.to_string_lossy().as_ref(),
            repo_path.to_string_lossy().as_ref(),
        ])
        .output()
        .expect("clone");
    fs::write(repo_path.join("local.txt"), "discard me\n").expect("dirty");
    let database = Database::open(temp.path().join("index.db")).expect("db");
    let repo = scanned_repo(&database, &root, &repo_path);
    let outcome = pull_repo(
        &repo_path,
        &repo,
        &policy(PullConflictAction::Overwrite),
        PullMode::Unattended,
    )
    .expect("overwrite");
    assert_eq!(outcome.result, "ok");
    assert!(!repo_path.join("local.txt").exists());
}

#[test]
fn non_conflict_git_failure_is_not_reported_as_user_conflict() {
    let temp = TempDir::new().expect("temp");
    let root = temp.path().join("root");
    let repo_path = root.join("network/demo");
    fs::create_dir_all(&repo_path).expect("repo");
    git(&repo_path, &["init", "-q", "-b", "main"]);
    git(
        &repo_path,
        &["config", "user.email", "test@example.invalid"],
    );
    git(&repo_path, &["config", "user.name", "Xingshu Test"]);
    fs::write(repo_path.join("README.md"), "clean\n").expect("file");
    git(&repo_path, &["add", "README.md"]);
    git(&repo_path, &["commit", "-q", "-m", "clean"]);
    git(
        &repo_path,
        &[
            "remote",
            "add",
            "origin",
            "file:///path/that/does/not/exist.git",
        ],
    );
    let database = Database::open(temp.path().join("index.db")).expect("db");
    let repo = scanned_repo(&database, &root, &repo_path);
    let error = pull_repo(
        &repo_path,
        &repo,
        &policy(PullConflictAction::Stop),
        PullMode::Interactive,
    )
    .expect_err("network failure");
    assert!(matches!(error, VcsError::GitCommand(_)));
}
