use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;
use xingshu_core::backup::{create_backup, list_backups, restore_backup};

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

fn create_repo(parent: &Path, name: &str) -> PathBuf {
    let path = parent.join(name);
    fs::create_dir_all(&path).expect("repo dir");
    git(&path, &["init", "-q", "-b", "main"]);
    git(&path, &["config", "user.email", "test@example.invalid"]);
    git(&path, &["config", "user.name", "Xingshu Test"]);
    fs::write(path.join("README.md"), "fixture\n").expect("fixture");
    git(&path, &["add", "README.md"]);
    git(&path, &["commit", "-q", "-m", "fixture"]);
    path
}

#[test]
fn directory_backup_create_list_and_restore() {
    let temp = TempDir::new().expect("temp");
    let repo = create_repo(temp.path(), "demo");
    let backup = create_backup(&repo, None, None, 99).expect("create");
    assert!(backup.is_dir());
    assert!(
        repo.join("README.md").is_file(),
        "source must stay in place"
    );

    let entries = list_backups(&repo, None).expect("list");
    assert_eq!(entries.len(), 1);
    assert!(!entries[0].is_archive);

    fs::write(repo.join("README.md"), "changed\n").expect("mutate");
    fs::write(repo.join("extra.txt"), "junk\n").expect("junk");
    restore_backup(&repo, &backup).expect("restore");
    assert_eq!(
        fs::read_to_string(repo.join("README.md")).expect("read"),
        "fixture\n"
    );
    assert!(
        !repo.join("extra.txt").exists(),
        "restore must replace state"
    );
    let has_pre_restore = fs::read_dir(temp.path())
        .expect("parent")
        .filter_map(Result::ok)
        .any(|entry| entry.file_name().to_string_lossy().contains("pre-restore"));
    assert!(has_pre_restore, "pre-restore snapshot missing");
}

#[test]
fn archive_backup_and_keep_pruning() {
    let temp = TempDir::new().expect("temp");
    let repo = create_repo(temp.path(), "demo");
    let first = create_backup(&repo, None, Some("tar.gz"), 1).expect("archive gz");
    assert!(first.is_file());
    std::thread::sleep(std::time::Duration::from_millis(5));
    let second = create_backup(&repo, None, Some("tar.zst"), 1).expect("archive zst");
    assert!(second.is_file());

    let entries = list_backups(&repo, None).expect("list");
    assert_eq!(
        entries.len(),
        1,
        "keep=1 should prune older backups: {entries:?}"
    );
    assert!(entries[0].is_archive);

    fs::remove_file(repo.join("README.md")).expect("remove");
    restore_backup(&repo, &entries[0].path).expect("restore archive");
    assert_eq!(
        fs::read_to_string(repo.join("README.md")).expect("read"),
        "fixture\n"
    );
}
