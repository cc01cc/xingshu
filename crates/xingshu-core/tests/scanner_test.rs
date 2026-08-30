use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Utc;
use tempfile::TempDir;
use xingshu_core::db::Database;
use xingshu_core::policy::ResolvedPolicy;
use xingshu_core::puller::{PullMode, pull_repo};
use xingshu_core::scanner::scan_roots;
use xingshu_core::types::{Root, ScanOptions, VcsError};

fn run_git(path: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .expect("git must be installed for scanner integration tests");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn create_repo(parent: &Path, name: &str) -> PathBuf {
    let path = parent.join(name);
    fs::create_dir_all(&path).expect("create repo directory");
    run_git(&path, &["init", "-q", "-b", "main"]);
    run_git(&path, &["config", "user.email", "test@example.invalid"]);
    run_git(&path, &["config", "user.name", "Xingshu Test"]);
    fs::write(path.join("README.md"), "fixture\n").expect("write fixture");
    run_git(&path, &["add", "README.md"]);
    run_git(&path, &["commit", "-q", "-m", "fixture"]);
    path
}

fn root_for(path: &Path) -> Root {
    let now = Utc::now();
    Root {
        id: None,
        name: "fixture".to_owned(),
        path: path.to_path_buf(),
        disk_label: None,
        mount_point: None,
        priority: 0,
        created_at: now,
        updated_at: now,
    }
}

fn scan(database: &Database, root: &Root, options: ScanOptions) {
    let id = database.upsert_root(root).expect("insert root");
    let mut registered = root.clone();
    registered.id = Some(id);
    scan_roots(database, &[registered], &options).expect("scan fixture");
}

#[test]
fn scanner_handles_empty_directories_bare_repositories_and_generated_backups() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().join("root");
    fs::create_dir_all(root.join("empty/deep/tree")).expect("empty tree");
    create_repo(&root.join("org"), "repo");
    Command::new("git")
        .args([
            "init",
            "--bare",
            "-q",
            "--",
            root.join("org/bare.git").to_string_lossy().as_ref(),
        ])
        .output()
        .expect("create bare repo");
    fs::create_dir_all(root.join("org/repo.bak.20260829/.git")).expect("backup marker");

    let database = Database::open(temp.path().join("index.db")).expect("open database");
    scan(&database, &root_for(&root), ScanOptions::default());
    let repos = database.list_repos().expect("list repos");

    assert_eq!(repos.len(), 2);
    assert!(repos.iter().any(|repo| repo.is_bare));
    assert!(
        repos
            .iter()
            .all(|repo| !repo.rel_path.to_string_lossy().contains("bak."))
    );
}

#[test]
fn scanner_skips_nested_repositories_by_default_and_can_include_them() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().join("root");
    let outer = create_repo(&root, "outer");
    create_repo(&outer.join("embedded"), "inner");
    let database = Database::open(temp.path().join("index.db")).expect("open database");
    let root_id = database.upsert_root(&root_for(&root)).expect("insert root");
    let mut registered = root_for(&root);
    registered.id = Some(root_id);

    scan_roots(&database, &[registered.clone()], &ScanOptions::default()).expect("scan outer");
    assert_eq!(database.list_repos().expect("list outer").len(), 1);

    let options = ScanOptions {
        include_nested: true,
        ..ScanOptions::default()
    };
    scan_roots(&database, &[registered], &options).expect("scan nested");
    assert_eq!(database.list_repos().expect("list nested").len(), 2);
}

#[test]
fn scanner_classifies_own_repositories_and_sanitizes_remote_credentials() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().join("root");
    let repo_path = create_repo(&root, "mine");
    run_git(
        &repo_path,
        &[
            "remote",
            "add",
            "origin",
            "https://token@example.com/cc01cc/mine.git",
        ],
    );
    let database = Database::open(temp.path().join("index.db")).expect("open database");
    let root_id = database.upsert_root(&root_for(&root)).expect("insert root");
    let mut registered = root_for(&root);
    registered.id = Some(root_id);
    let mut options = ScanOptions::default();
    options.my_orgs.push("cc01cc".to_owned());
    scan_roots(&database, &[registered], &options).expect("scan own repo");

    let repo = database
        .list_repos()
        .expect("list repos")
        .pop()
        .expect("repo");
    assert_eq!(repo.repo_kind.as_str(), "own");
    assert_eq!(
        repo.remote_url.as_deref(),
        Some("https://example.com/cc01cc/mine.git")
    );
}

#[test]
fn scanner_prioritizes_upstream_remote_over_my_org_classification_for_forks() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().join("root");
    let repo_path = create_repo(&root, "fork");
    run_git(
        &repo_path,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/cc01cc/fork.git",
        ],
    );
    run_git(
        &repo_path,
        &[
            "remote",
            "add",
            "upstream",
            "https://github.com/example/fork.git",
        ],
    );
    let database = Database::open(temp.path().join("index.db")).expect("open database");
    let root_id = database.upsert_root(&root_for(&root)).expect("insert root");
    let mut registered = root_for(&root);
    registered.id = Some(root_id);
    let options = ScanOptions {
        my_orgs: vec!["cc01cc".to_owned()],
        ..ScanOptions::default()
    };
    scan_roots(&database, &[registered], &options).expect("scan fork");
    let repo = database
        .list_repos()
        .expect("list repos")
        .pop()
        .expect("repo");
    assert_eq!(repo.repo_kind.as_str(), "fork");
    assert_eq!(repo.push_protect_remote.as_deref(), Some("upstream"));
}

#[test]
fn dirty_repository_requires_interactive_decision_and_aborts_unattended() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().join("root");
    let repo_path = create_repo(&root, "dirty");
    fs::write(repo_path.join("local.txt"), "uncommitted\n").expect("dirty fixture");
    let database = Database::open(temp.path().join("index.db")).expect("open database");
    let root_id = database.upsert_root(&root_for(&root)).expect("insert root");
    let mut registered = root_for(&root);
    registered.id = Some(root_id);
    scan_roots(&database, &[registered], &ScanOptions::default()).expect("scan dirty repo");
    let repo = database
        .list_repos()
        .expect("list repos")
        .pop()
        .expect("repo");
    let policy = ResolvedPolicy::default();

    let error =
        pull_repo(&repo_path, &repo, &policy, PullMode::Interactive).expect_err("must stop");
    assert!(matches!(error, VcsError::ConflictNeedsDecision));
    let outcome = pull_repo(&repo_path, &repo, &policy, PullMode::Unattended).expect("must abort");
    assert_eq!(outcome.result, "aborted");
}
