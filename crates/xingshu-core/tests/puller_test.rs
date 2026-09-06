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
fn bare_repository_is_skipped_by_batch_pull() {
    let temp = TempDir::new().expect("temp");
    let root = temp.path().join("root");
    let repo_path = root.join("archives/demo.git");
    fs::create_dir_all(repo_path.parent().expect("parent")).expect("parent");
    let output = Command::new("git")
        .args(["init", "--bare", "-q", repo_path.to_string_lossy().as_ref()])
        .output()
        .expect("bare repo");
    assert!(output.status.success());
    let database = Database::open(temp.path().join("index.db")).expect("db");
    let repo = scanned_repo(&database, &root, &repo_path);
    let outcome = pull_repo(
        &repo_path,
        &repo,
        &policy(PullConflictAction::Abort),
        PullMode::Unattended,
    )
    .expect("bare pull");
    assert_eq!(outcome.result, "skipped");
}

#[test]
fn dirty_conflict_carries_status_entries() {
    let temp = TempDir::new().expect("temp");
    let (remote, _) = seed_remote(&temp);
    let root = temp.path().join("root");
    let repo_path = root.join("network/demo");
    fs::create_dir_all(repo_path.parent().expect("parent")).expect("parent");
    Command::new("git")
        .args(["clone", "-q", remote.to_string_lossy().as_ref(), repo_path.to_string_lossy().as_ref()])
        .output()
        .expect("clone");
    fs::write(repo_path.join("README.md"), "changed\n").expect("tracked");
    fs::write(repo_path.join("untracked.txt"), "new\n").expect("untracked");
    let database = Database::open(temp.path().join("index.db")).expect("db");
    let repo = scanned_repo(&database, &root, &repo_path);
    let outcome = pull_repo(
        &repo_path,
        &repo,
        &policy(PullConflictAction::Abort),
        PullMode::Unattended,
    )
    .expect("abort");
    assert_eq!(outcome.result, "aborted");
    let info = outcome.conflict.expect("conflict info");
    assert_eq!(info.kind, xingshu_core::ConflictKind::Dirty);
    assert_eq!(info.untracked, 1);
    assert_eq!(info.modified, 1);
    assert!(info.status_entries.iter().any(|entry| entry.path.contains("untracked.txt")));
    assert!(repo_path.join("untracked.txt").exists());
}

#[test]
fn upstream_rollback_ahead_clean_is_not_a_conflict() {
    let temp = TempDir::new().expect("temp");
    let (remote, seed) = seed_remote(&temp);
    let root = temp.path().join("root");
    let repo_path = root.join("network/demo");
    fs::create_dir_all(repo_path.parent().expect("parent")).expect("parent");
    Command::new("git")
        .args(["clone", "-q", remote.to_string_lossy().as_ref(), repo_path.to_string_lossy().as_ref()])
        .output()
        .expect("clone");
    git(&seed, &["config", "user.email", "test@example.invalid"]);
    git(&seed, &["config", "user.name", "Xingshu Test"]);
    fs::write(seed.join("upstream.txt"), "upstream\n").expect("file");
    git(&seed, &["add", "upstream.txt"]);
    git(&seed, &["commit", "-q", "-m", "upstream commit"]);
    git(&seed, &["push", "-q"]);
    git(&repo_path, &["pull", "-q", "--ff-only"]);
    fs::write(repo_path.join("local.txt"), "local ahead\n").expect("file");
    git(&repo_path, &["config", "user.email", "test@example.invalid"]);
    git(&repo_path, &["config", "user.name", "Xingshu Test"]);
    git(&repo_path, &["add", "local.txt"]);
    git(&repo_path, &["commit", "-q", "-m", "local ahead commit"]);
    // 上游 force-push 回退（丢弃 upstream commit），本地保持 ahead 2 干净
    git(&seed, &["reset", "-q", "--hard", "HEAD~1"]);
    git(&seed, &["push", "-q", "--force"]);
    let database = Database::open(temp.path().join("index.db")).expect("db");
    let repo = scanned_repo(&database, &root, &repo_path);
    let outcome = pull_repo(
        &repo_path,
        &repo,
        &policy(PullConflictAction::Abort),
        PullMode::Unattended,
    )
    .expect("ahead pull should not conflict");
    assert_eq!(outcome.result, "ahead");
    let info = outcome.conflict.expect("ahead info");
    assert_eq!(info.ahead, 2);
    assert_eq!(info.behind, 0);
    assert!(repo_path.join("local.txt").exists());
}

#[test]
fn diverged_conflict_carries_dual_columns_and_equivalent() {
    let temp = TempDir::new().expect("temp");
    let (remote, seed) = seed_remote(&temp);
    let root = temp.path().join("root");
    let repo_path = root.join("network/demo");
    fs::create_dir_all(repo_path.parent().expect("parent")).expect("parent");
    Command::new("git")
        .args(["clone", "-q", remote.to_string_lossy().as_ref(), repo_path.to_string_lossy().as_ref()])
        .output()
        .expect("clone");
    // 本地领先 1：内容与上游重做提交相同（模拟回退重做）
    // 前置：上游先加 A2 commit 并本地同步，保证 reset HEAD~1 有父可回退
    git(&seed, &["config", "user.email", "test@example.invalid"]);
    git(&seed, &["config", "user.name", "Xingshu Test"]);
    fs::write(seed.join("upstream.txt"), "upstream\n").expect("file");
    git(&seed, &["add", "upstream.txt"]);
    git(&seed, &["commit", "-q", "-m", "upstream commit"]);
    git(&seed, &["push", "-q"]);
    git(&repo_path, &["pull", "-q", "--ff-only"]);
    fs::write(repo_path.join("feature.txt"), "same content\n").expect("file");
    git(&repo_path, &["config", "user.email", "test@example.invalid"]);
    git(&repo_path, &["config", "user.name", "Xingshu Test"]);
    git(&repo_path, &["add", "feature.txt"]);
    git(&repo_path, &["commit", "-q", "-m", "add feature line"]);
    // 上游 force-push 重走：回退 seed 提交后提交相同内容（hash 不同）
    git(&seed, &["config", "user.email", "test@example.invalid"]);
    git(&seed, &["config", "user.name", "Xingshu Test"]);
    git(&seed, &["reset", "-q", "--hard", "HEAD~1"]);
    fs::write(seed.join("feature.txt"), "same content\n").expect("file");
    git(&seed, &["add", "feature.txt"]);
    git(&seed, &["commit", "-q", "-m", "reapply feature (upstream redo)"]);
    git(&seed, &["push", "-q", "--force"]);
    let database = Database::open(temp.path().join("index.db")).expect("db");
    let repo = scanned_repo(&database, &root, &repo_path);
    let outcome = pull_repo(
        &repo_path,
        &repo,
        &policy(PullConflictAction::Abort),
        PullMode::Unattended,
    )
    .expect("diverged");
    assert_eq!(outcome.result, "aborted");
    let info = outcome.conflict.expect("diverged info");
    // 本地领先 2（A2 upstream commit + B feature commit，因上游回退了 A2），
    // 上游领先 1（C redo commit）
    assert_eq!(info.ahead, 2);
    assert_eq!(info.behind, 1);
    assert_eq!(info.local_only.len(), 2);
    assert_eq!(info.upstream_only.len(), 1);
    assert!(info.merge_base.is_some());
    assert_eq!(info.equivalent.len(), 1, "expected patch-equivalent pair");
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
