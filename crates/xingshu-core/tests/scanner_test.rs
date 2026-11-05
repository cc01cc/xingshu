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
fn dirty_repository_requires_interactive_decision_and_skips_unattended() {
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
    assert!(matches!(error, VcsError::ConflictNeedsDecision(_)));
    let outcome = pull_repo(&repo_path, &repo, &policy, PullMode::Unattended).expect("must skip");
    assert_eq!(outcome.result, "skipped");
}

#[test]
fn incremental_scan_skips_unchanged_repositories() {
    let temp = TempDir::new().expect("temp dir");
    let root = temp.path().join("root");
    let repo_path = create_repo(&root, "keep");
    let database = Database::open(temp.path().join("index.db")).expect("open database");
    let root_id = database.upsert_root(&root_for(&root)).expect("insert root");
    let mut registered = root_for(&root);
    registered.id = Some(root_id);

    let first =
        scan_roots(&database, &[registered.clone()], &ScanOptions::default()).expect("first scan");
    assert_eq!(first.repos_found, 1);
    assert_eq!(first.unchanged_repos, 0);

    let second =
        scan_roots(&database, &[registered.clone()], &ScanOptions::default()).expect("second scan");
    assert_eq!(second.repos_found, 1);
    assert_eq!(second.unchanged_repos, 1);

    // A git-level change invalidates the fingerprint.
    fs::write(repo_path.join("README.md"), "changed\n").expect("change fixture");
    run_git(&repo_path, &["add", "README.md"]);
    run_git(&repo_path, &["commit", "-q", "-m", "change"]);
    let third = scan_roots(&database, &[registered], &ScanOptions::default()).expect("third scan");
    assert_eq!(third.unchanged_repos, 0);
    let repo = database.list_repos().expect("repos").pop().expect("repo");
    assert!(repo.scan_fingerprint.is_some());
    assert!(!repo.dirty);
}

#[test]
fn scanner_records_remote_host_namespace_and_working_state() {
    let temp = TempDir::new().expect("temp dir");
    let root_a = temp.path().join("root-a");
    let root_b = temp.path().join("root-b");
    let repo_a = create_repo(&root_a, "bar");
    run_git(
        &repo_a,
        &["remote", "add", "origin", "git@github.com:foo/bar.git"],
    );
    let repo_b = create_repo(&root_b, "bar");
    run_git(
        &repo_b,
        &["remote", "add", "origin", "https://gitlab.com/foo/bar.git"],
    );
    fs::write(repo_b.join("dirty.txt"), "x\n").expect("dirty fixture");

    let database = Database::open(temp.path().join("index.db")).expect("open database");
    let id_a = database.upsert_root(&root_for(&root_a)).expect("root a");
    let mut registered_a = root_for(&root_a);
    registered_a.id = Some(id_a);
    let mut root_b_record = root_for(&root_b);
    root_b_record.name = "fixture-b".to_owned();
    let id_b = database.upsert_root(&root_b_record).expect("root b");
    let mut registered_b = root_b_record;
    registered_b.id = Some(id_b);

    scan_roots(
        &database,
        &[registered_a, registered_b],
        &ScanOptions::default(),
    )
    .expect("scan");

    let repos = database.list_repos().expect("repos");
    let github = repos
        .iter()
        .find(|repo| repo.remote_host.as_deref() == Some("github.com"))
        .expect("github repo");
    assert_eq!(github.remote_namespace.as_deref(), Some("foo"));
    assert_eq!(github.org, "foo");
    assert_eq!(github.name, "bar");
    assert_eq!(github.org_source.as_deref(), Some("remote"));
    assert!(!github.dirty);
    assert!(github.status_checked_at.is_some());

    let gitlab = repos
        .iter()
        .find(|repo| repo.remote_host.as_deref() == Some("gitlab.com"))
        .expect("gitlab repo");
    assert!(gitlab.dirty);
    assert_eq!(gitlab.ahead, 0);
    assert_eq!(gitlab.behind, 0);

    // Host-qualified lookup disambiguates identical org/name across hosts.
    let found = database
        .find_repo("github.com/foo/bar")
        .expect("find github")
        .expect("github repo");
    assert_eq!(found.remote_host.as_deref(), Some("github.com"));
    let found = database
        .find_repo("gitlab.com/foo/bar")
        .expect("find gitlab")
        .expect("gitlab repo");
    assert_eq!(found.remote_host.as_deref(), Some("gitlab.com"));
}
