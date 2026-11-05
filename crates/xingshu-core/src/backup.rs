//! Independent repository backups (PLAN-297 ISS-03).
//!
//! Unlike the in-pull `backup` conflict action (which renames the repository
//! and re-clones it), these helpers create a *copy* next to the repository or
//! in a configured destination. The source repository is never moved.
//!
//! Two formats are supported: a plain directory snapshot and a system-`tar`
//! archive (`.tar.gz` / `.tar.zst`, chosen by file suffix via `tar -a`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Utc;

use crate::types::VcsError;

#[derive(Debug, Clone, serde::Serialize)]
pub struct BackupEntry {
    pub path: PathBuf,
    pub is_archive: bool,
    pub bytes: u64,
    pub created_at: String,
}

/// Create a copy of `repo_path` in `dest` (defaults to the parent directory).
///
/// `archive` is `None` for a directory snapshot, or a suffix such as
/// `tar.gz` / `tar.zst` for a system-`tar` archive. `keep` prunes older
/// snapshots of the same repository once the new one is complete.
pub fn create_backup(
    repo_path: &Path,
    dest: Option<&Path>,
    archive: Option<&str>,
    keep: usize,
) -> Result<PathBuf, VcsError> {
    if !repo_path.is_dir() {
        return Err(VcsError::Filesystem(format!(
            "repository not found: {}",
            repo_path.display()
        )));
    }
    let name = repo_path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .ok_or_else(|| VcsError::Filesystem("repository has no name".to_owned()))?;
    let dest = dest.map(Path::to_path_buf).unwrap_or_else(|| {
        repo_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    });
    if !dest.is_dir() {
        return Err(VcsError::Filesystem(format!(
            "backup destination not found: {}",
            dest.display()
        )));
    }
    let stamp = Utc::now().format("%Y%m%d%H%M%S%.3f");
    let base = format!("{name}.bak.{stamp}");
    match archive {
        None => {
            let target = dest.join(&base);
            copy_dir(repo_path, &target)?;
            prune_backups(repo_path, Some(&dest), keep, true)?;
            Ok(target)
        }
        Some(suffix) => {
            let file = dest.join(format!("{base}.{suffix}"));
            let parent = repo_path
                .parent()
                .ok_or_else(|| VcsError::Filesystem("repository has no parent".to_owned()))?;
            let status = Command::new("tar")
                .arg("-a")
                .arg("-cf")
                .arg(&file)
                .arg("-C")
                .arg(parent)
                .arg(&name)
                .status()
                .map_err(|error| VcsError::Filesystem(format!("tar failed to start: {error}")))?;
            if !status.success() {
                let _ = fs::remove_file(&file);
                return Err(VcsError::Filesystem(format!(
                    "tar exited with {status} while archiving {}",
                    repo_path.display()
                )));
            }
            prune_backups(repo_path, Some(&dest), keep, true)?;
            Ok(file)
        }
    }
}

/// List snapshots of `repo_path` in `dest` (defaults to the parent directory).
pub fn list_backups(repo_path: &Path, dest: Option<&Path>) -> Result<Vec<BackupEntry>, VcsError> {
    let name = repo_path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .ok_or_else(|| VcsError::Filesystem("repository has no name".to_owned()))?;
    let dest = dest.map(Path::to_path_buf).unwrap_or_else(|| {
        repo_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    });
    let prefix = format!("{name}.bak.");
    let mut entries = Vec::new();
    if let Ok(read_dir) = fs::read_dir(&dest) {
        for entry in read_dir.filter_map(Result::ok) {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if !file_name.starts_with(&prefix) {
                continue;
            }
            let path = entry.path();
            let is_archive = path.is_file();
            let bytes = if is_archive {
                entry.metadata().map(|meta| meta.len()).unwrap_or(0)
            } else {
                0
            };
            entries.push(BackupEntry {
                created_at: file_name
                    .strip_prefix(&prefix)
                    .unwrap_or(&file_name)
                    .to_owned(),
                path,
                is_archive,
                bytes,
            });
        }
    }
    entries.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(entries)
}

/// Restore `backup` over `repo_path`. An existing repository is first moved
/// to a sibling `*.bak.pre-restore.*` directory so the operation is
/// recoverable; if the restore fails the original is moved back.
pub fn restore_backup(repo_path: &Path, backup: &Path) -> Result<(), VcsError> {
    if !backup.exists() {
        return Err(VcsError::Filesystem(format!(
            "backup not found: {}",
            backup.display()
        )));
    }
    let name = repo_path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .ok_or_else(|| VcsError::Filesystem("repository has no name".to_owned()))?;
    let parent = repo_path
        .parent()
        .ok_or_else(|| VcsError::Filesystem("repository has no parent".to_owned()))?;
    let pre_restore = if repo_path.exists() {
        let stamp = Utc::now().format("%Y%m%d%H%M%S%.3f");
        let moved = parent.join(format!("{name}.bak.pre-restore.{stamp}"));
        fs::rename(repo_path, &moved)
            .map_err(|error| VcsError::Filesystem(format!("pre-restore move failed: {error}")))?;
        Some(moved)
    } else {
        None
    };

    let result: Result<(), VcsError> = if backup.is_dir() {
        fs::rename(backup, repo_path)
            .map_err(|error| VcsError::Filesystem(format!("restore failed: {error}")))
    } else {
        let status = Command::new("tar")
            .arg("-a")
            .arg("-xf")
            .arg(backup)
            .arg("-C")
            .arg(parent)
            .status()
            .map_err(|error| VcsError::Filesystem(format!("tar failed to start: {error}")))?;
        if status.success() && repo_path.is_dir() {
            Ok(())
        } else {
            Err(VcsError::Filesystem(format!(
                "tar exited with {status} while restoring {}",
                backup.display()
            )))
        }
    };

    if result.is_err() {
        if let Some(moved) = pre_restore
            && !repo_path.exists()
        {
            let _ = fs::rename(&moved, repo_path);
        }
        return result;
    }
    Ok(())
}

/// Prune older backups, keeping the newest `keep` entries.
///
/// Returns the paths that were (or with `execute = false`, would be) removed,
/// oldest first. With `execute = false` this is a pure preview: the
/// filesystem is not touched (PLAN-0314).
pub fn prune_backups(
    repo_path: &Path,
    dest: Option<&Path>,
    keep: usize,
    execute: bool,
) -> Result<Vec<PathBuf>, VcsError> {
    let entries = list_backups(repo_path, dest)?;
    if entries.len() <= keep {
        return Ok(Vec::new());
    }
    let remove_count = entries.len() - keep;
    let mut affected = Vec::new();
    for entry in entries.into_iter().take(remove_count) {
        affected.push(entry.path.clone());
        if execute {
            if entry.is_archive {
                fs::remove_file(&entry.path).map_err(|error| {
                    VcsError::Filesystem(format!("prune {}: {error}", entry.path.display()))
                })?;
            } else {
                fs::remove_dir_all(&entry.path).map_err(|error| {
                    VcsError::Filesystem(format!("prune {}: {error}", entry.path.display()))
                })?;
            }
        }
    }
    Ok(affected)
}

fn copy_dir(source: &Path, target: &Path) -> Result<(), VcsError> {
    fs::create_dir_all(target)
        .map_err(|error| VcsError::Filesystem(format!("create {}: {error}", target.display())))?;
    for entry in fs::read_dir(source)
        .map_err(|error| VcsError::Filesystem(format!("read {}: {error}", source.display())))?
        .filter_map(Result::ok)
    {
        let from = entry.path();
        let to = target.join(entry.file_name());
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|error| {
                VcsError::Filesystem(format!("copy {}: {error}", from.display()))
            })?;
        }
    }
    Ok(())
}
