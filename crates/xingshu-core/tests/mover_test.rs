use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use xingshu_core::mover::{move_repo, move_repo_with_available_space};

fn repo(path: &Path) {
    fs::create_dir_all(path).expect("repo");
    let output = Command::new("git")
        .args(["-C", path.to_string_lossy().as_ref(), "init", "-q"])
        .output()
        .expect("git");
    assert!(output.status.success());
}

#[test]
fn mover_rejects_same_root() {
    let temp = TempDir::new().expect("temp");
    let root = temp.path().join("root");
    let source = root.join("org/repo");
    repo(&source);
    let error = move_repo(&root, &source, &root, Path::new("org/repo")).expect_err("same root");
    assert!(error.to_string().contains("same-root"));
}

#[test]
fn mover_relocates_repository_between_staging_roots() {
    let temp = TempDir::new().expect("temp");
    let source_root = temp.path().join("root-a");
    let target_root = temp.path().join("root-b");
    fs::create_dir_all(&target_root).expect("target");
    let source = source_root.join("org/repo");
    repo(&source);
    let destination =
        move_repo(&source_root, &source, &target_root, Path::new("org/repo")).expect("move");
    assert!(destination.join(".git").is_dir());
    assert!(!source.exists());
}

#[test]
fn mover_rejects_existing_destination() {
    let temp = TempDir::new().expect("temp");
    let source_root = temp.path().join("root-a");
    let target_root = temp.path().join("root-b");
    fs::create_dir_all(target_root.join("org/repo/.git")).expect("destination");
    let source = source_root.join("org/repo");
    repo(&source);
    let error = move_repo(&source_root, &source, &target_root, Path::new("org/repo"))
        .expect_err("destination conflict");
    assert!(error.to_string().contains("already exists"));
}

#[test]
fn mover_rejects_insufficient_injected_space() {
    let temp = TempDir::new().expect("temp");
    let source_root = temp.path().join("root-a");
    let target_root = temp.path().join("root-b");
    fs::create_dir_all(&target_root).expect("target");
    let source = source_root.join("org/repo");
    repo(&source);
    let error = move_repo_with_available_space(
        &source_root,
        &source,
        &target_root,
        Path::new("org/repo"),
        0,
    )
    .expect_err("insufficient space");
    assert!(error.to_string().contains("insufficient space"));
}
