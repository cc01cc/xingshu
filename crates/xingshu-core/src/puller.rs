use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use chrono::Utc;

use crate::git::{self, GitCommandOutput};
use crate::policy::{PullConflictAction, ResolvedPolicy, is_update_enabled};
use crate::progress::ProgressReporter;
use crate::types::{FetchLog, RepoRecord, VcsError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullMode {
    Interactive,
    Unattended,
}

#[derive(Debug, Clone)]
pub struct PullOutcome {
    pub result: String,
    pub backup_path: Option<PathBuf>,
    pub output: Option<GitCommandOutput>,
}

pub fn pull_repo(
    path: &Path,
    repo: &RepoRecord,
    policy: &ResolvedPolicy,
    mode: PullMode,
) -> Result<PullOutcome, VcsError> {
    pull_repo_with_reporter(path, repo, policy, mode, None)
}

pub fn pull_repo_with_reporter(
    path: &Path,
    repo: &RepoRecord,
    policy: &ResolvedPolicy,
    mode: PullMode,
    reporter: Option<&dyn ProgressReporter>,
) -> Result<PullOutcome, VcsError> {
    let started = Instant::now();
    tracing::debug!(
        path = %path.display(),
        repo_id = repo.id.unwrap_or_default(),
        strategy = %policy.pull_strategy,
        "pull requested"
    );
    let result = (|| {
        if repo.is_bare
            || !is_update_enabled(&policy.pull_strategy)
            || repo.clone_status == "broken"
        {
            return Ok(PullOutcome {
                result: "skipped".to_owned(),
                backup_path: None,
                output: None,
            });
        }

        let metadata = git::inspect(path)?;
        let dirty = metadata.dirty || metadata.ahead_of_upstream;
        let pull_result = if dirty {
            Err(VcsError::ConflictNeedsDecision)
        } else {
            git::run_git(path, &["pull", "--ff-only"])
        };

        match pull_result {
            Ok(output) => Ok(PullOutcome {
                result: "ok".to_owned(),
                backup_path: None,
                output: Some(output),
            }),
            Err(VcsError::ConflictNeedsDecision) => {
                let action = match mode {
                    PullMode::Interactive => policy.conflict_action,
                    PullMode::Unattended => policy.unattended_action,
                };
                resolve_conflict(path, repo, action)
            }
            Err(VcsError::GitCommand(message)) if is_conflict_message(&message) => {
                let action = match mode {
                    PullMode::Interactive => policy.conflict_action,
                    PullMode::Unattended => policy.unattended_action,
                };
                resolve_conflict(path, repo, action)
            }
            Err(error) => Err(error),
        }
    })();
    match &result {
        Ok(outcome) => tracing::info!(
            repo_id = repo.id.unwrap_or_default(),
            result = %outcome.result,
            duration_ms = started.elapsed().as_millis() as u64,
            "pull completed"
        ),
        Err(error) => tracing::error!(
            repo_id = repo.id.unwrap_or_default(),
            duration_ms = started.elapsed().as_millis() as u64,
            error = %error,
            "pull failed"
        ),
    }
    if let Some(reporter) = reporter {
        let elapsed = started.elapsed().as_millis() as u64;
        match &result {
            Ok(outcome) => {
                reporter.item_finished(&path.to_string_lossy(), &outcome.result, Some(elapsed))
            }
            Err(error) => reporter.item_finished(
                &path.to_string_lossy(),
                &format!("error: {error}"),
                Some(elapsed),
            ),
        }
    }
    result
}

fn is_conflict_message(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    [
        "cannot fast-forward",
        "not possible to fast-forward",
        "non-fast-forward",
        "divergent branches",
        "would be overwritten",
    ]
    .iter()
    .any(|marker| message.contains(marker))
}

pub fn make_fetch_log(repo_id: i64, strategy: &str, outcome: &PullOutcome) -> FetchLog {
    let now = Utc::now();
    FetchLog {
        id: None,
        repo_id,
        started_at: now,
        finished_at: Some(now),
        strategy: strategy.to_owned(),
        result: outcome.result.clone(),
        objects: None,
        bytes: outcome
            .output
            .as_ref()
            .map(|output| output.stdout.len() as u64),
        error: None,
        created_at: now,
        updated_at: now,
    }
}

fn resolve_conflict(
    path: &Path,
    repo: &RepoRecord,
    action: PullConflictAction,
) -> Result<PullOutcome, VcsError> {
    match action {
        PullConflictAction::Stop => Err(VcsError::ConflictNeedsDecision),
        PullConflictAction::Abort => Ok(PullOutcome {
            result: "aborted".to_owned(),
            backup_path: None,
            output: None,
        }),
        PullConflictAction::Backup => {
            let backup = backup_path(path);
            fs::rename(path, &backup).map_err(|error| {
                VcsError::Filesystem(format!("backup {}: {error}", path.display()))
            })?;
            let remote = repo.remote_url.as_deref().ok_or_else(|| {
                VcsError::GitCommand("cannot re-clone without origin remote".to_owned())
            });
            match remote {
                Ok(remote) => {
                    if let Err(error) = clone_into(remote, path) {
                        return Err(VcsError::Filesystem(format!(
                            "re-clone failed after backup {}: {error}",
                            backup.display()
                        )));
                    }
                    Ok(PullOutcome {
                        result: "ok".to_owned(),
                        backup_path: Some(backup),
                        output: None,
                    })
                }
                Err(error) => Err(error),
            }
        }
        PullConflictAction::Overwrite => {
            git::run_git(path, &["reset", "--hard"])?;
            git::run_git(path, &["clean", "-fd"])?;
            let output = git::run_git(path, &["pull", "--ff-only"])?;
            Ok(PullOutcome {
                result: "ok".to_owned(),
                backup_path: None,
                output: Some(output),
            })
        }
    }
}

fn backup_path(path: &Path) -> PathBuf {
    let timestamp = Utc::now().format("%Y%m%d%H%M%S");
    PathBuf::from(format!("{}.bak.{}", path.display(), timestamp))
}

fn clone_into(remote: &str, path: &Path) -> Result<(), VcsError> {
    let parent = path
        .parent()
        .ok_or_else(|| VcsError::Filesystem(format!("{} has no parent", path.display())))?;
    let name = path
        .file_name()
        .ok_or_else(|| VcsError::Filesystem(format!("{} has no name", path.display())))?;
    let output = std::process::Command::new("git")
        .arg("clone")
        .arg(remote)
        .arg(name)
        .current_dir(parent)
        .output()
        .map_err(|error| VcsError::GitCommand(error.to_string()))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(VcsError::GitCommand(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
}
