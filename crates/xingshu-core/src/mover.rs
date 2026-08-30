use std::fs;
use std::path::{Path, PathBuf};

use crate::git;
use crate::types::VcsError;

pub fn move_repo(
    source_root: &Path,
    source: &Path,
    target_root: &Path,
    relative: &Path,
) -> Result<PathBuf, VcsError> {
    let available = available_space(target_root)?;
    move_repo_with_available_space(source_root, source, target_root, relative, available)
}

pub fn move_repo_with_available_space(
    source_root: &Path,
    source: &Path,
    target_root: &Path,
    relative: &Path,
    available: u64,
) -> Result<PathBuf, VcsError> {
    if source_root == target_root {
        return Err(VcsError::Filesystem(
            "same-root move is not allowed".to_owned(),
        ));
    }
    let destination = target_root.join(relative);
    if destination.exists() {
        return Err(VcsError::Filesystem(format!(
            "destination already exists: {}",
            destination.display()
        )));
    }
    if !target_root.is_dir() {
        return Err(VcsError::Filesystem(format!(
            "target root does not exist: {}",
            target_root.display()
        )));
    }
    let required = directory_size(source);
    if available <= required {
        return Err(VcsError::Filesystem(format!(
            "target root has insufficient space: need {required}, available {available}"
        )));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| VcsError::Filesystem(error.to_string()))?;
    }
    fs::rename(source, &destination)
        .map_err(|error| VcsError::Filesystem(format!("move {}: {error}", source.display())))?;
    let valid = if git::is_bare_layout(&destination) {
        git::run_git(&destination, &["rev-parse", "--is-bare-repository"])
            .map(|output| output.stdout.trim() == "true")
            .unwrap_or(false)
    } else {
        destination.join(".git").exists()
    };
    if !valid {
        return Err(VcsError::Filesystem(format!(
            "moved repository failed validation: {}",
            destination.display()
        )));
    }
    Ok(destination)
}

fn directory_size(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| {
            let child = entry.path();
            if child.is_dir() {
                directory_size(&child)
            } else {
                entry.metadata().map(|metadata| metadata.len()).unwrap_or(0)
            }
        })
        .sum()
}

fn available_space(path: &Path) -> Result<u64, VcsError> {
    let drive = path
        .to_string_lossy()
        .chars()
        .next()
        .filter(|character| character.is_ascii_alphabetic())
        .ok_or_else(|| {
            VcsError::Filesystem(format!("cannot determine drive: {}", path.display()))
        })?;
    let command = format!("(Get-PSDrive -Name '{}').Free", drive);
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", command.as_str()])
        .output()
        .map_err(|error| VcsError::Filesystem(error.to_string()))?;
    if !output.status.success() {
        return Err(VcsError::Filesystem(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .map_err(|error| VcsError::Filesystem(format!("invalid free space: {error}")))
}
