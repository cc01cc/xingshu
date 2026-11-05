use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use xingshu_core::git::{DIFF_MAX_BYTES, DiffKind, diff_text};

fn git(path: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repo(dir: &Path) {
    std::fs::create_dir_all(dir).expect("dir");
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "test@example.invalid"]);
    git(dir, &["config", "user.name", "Xingshu Test"]);
}

#[test]
fn reports_no_head_for_unborn_repository() {
    let temp = TempDir::new().expect("temp");
    let repo = temp.path().join("repo");
    init_repo(&repo);
    std::fs::write(repo.join("README.md"), "uncommitted\n").expect("write");
    let diff = diff_text(&repo, "README.md").expect("diff");
    assert_eq!(diff.kind, DiffKind::NoHead);
    assert_eq!(diff.patch, "");
}

#[test]
fn reports_binary_modification() {
    let temp = TempDir::new().expect("temp");
    let repo = temp.path().join("repo");
    init_repo(&repo);
    std::fs::write(repo.join("blob.bin"), [0u8, 1, 2, 3]).expect("write");
    git(&repo, &["add", "blob.bin"]);
    git(&repo, &["commit", "-q", "-m", "binary"]);
    std::fs::write(repo.join("blob.bin"), [0u8, 9, 9, 9]).expect("modify");
    let diff = diff_text(&repo, "blob.bin").expect("diff");
    assert_eq!(diff.kind, DiffKind::Binary);
    assert_eq!(diff.patch, "");
    assert!(!diff.truncated);
}

#[test]
fn truncates_large_patch_at_char_boundary() {
    let temp = TempDir::new().expect("temp");
    let repo = temp.path().join("repo");
    init_repo(&repo);
    std::fs::write(repo.join("big.txt"), "start\n").expect("write");
    git(&repo, &["add", "big.txt"]);
    git(&repo, &["commit", "-q", "-m", "big"]);
    let mut content = String::from("start\n");
    for index in 0..80_000 {
        content.push_str(&format!("变更行 {index}\n"));
    }
    std::fs::write(repo.join("big.txt"), content).expect("modify");
    let diff = diff_text(&repo, "big.txt").expect("diff");
    assert_eq!(diff.kind, DiffKind::Diff);
    assert!(diff.truncated);
    assert!(diff.patch.len() <= DIFF_MAX_BYTES);
    assert!(diff.patch.is_char_boundary(diff.patch.len()));
}

#[test]
fn reports_missing_untracked_path_as_not_found() {
    let temp = TempDir::new().expect("temp");
    let repo = temp.path().join("repo");
    init_repo(&repo);
    std::fs::write(repo.join("README.md"), "seed\n").expect("write");
    git(&repo, &["add", "README.md"]);
    git(&repo, &["commit", "-q", "-m", "seed"]);
    let error = diff_text(&repo, "missing.txt").expect_err("missing path");
    assert!(error.to_string().contains("not found"), "{error}");
}
