use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use chrono::Utc;

use crate::git::{self, GitCommandOutput};
use crate::policy::{PullConflictAction, ResolvedPolicy, is_update_enabled};
use crate::progress::ProgressReporter;
use crate::types::{ConflictInfo, FetchLog, RepoRecord, VcsError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullMode {
    Interactive,
    Unattended,
}

#[derive(Debug, Clone)]
pub struct PullOutcome {
    pub result: String,
    pub backup_path: Option<PathBuf>,
    pub conflict: Option<ConflictInfo>,
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
                conflict: None,
                output: None,
            });
        }

        let metadata = git::inspect(path)?;
        if metadata.dirty {
            // 工作区有未提交变更：直接进冲突决策，携带 status 明细
            let info = dirty_conflict_info(&metadata);
            return resolve_pull_conflict_with_info(path, repo, policy, mode, Some(info));
        }
        // 工作区干净：先 fetch 比对 refs，区分 ahead_clean / ff / diverged·non-ff
        let fetch_result = git::run_git(path, &["fetch", "origin", "--prune"]);
        let divergence = match fetch_result {
            Ok(_) => git::divergence(path),
            Err(error) => {
                // 离线：回退旧判定——ahead 视为冲突（信息可能过期），干净则直接 pull
                tracing::warn!(error = %error, "fetch failed; falling back to offline heuristic");
                if metadata.ahead_of_upstream {
                    return resolve_pull_conflict_with_info(path, repo, policy, mode, None);
                }
                None
            }
        };
        match divergence {
            Some(info) if info.behind == 0 && info.ahead > 0 => {
                // ahead_clean：本地领先且上游无新提交；ff-only pull 为 no-op 成功
                // 不阻塞流程，结果记 ahead（UI 徽标数据源）
                tracing::info!(
                    ahead = info.ahead,
                    "pull skipped: local is ahead of upstream (possible upstream rollback)"
                );
                return Ok(PullOutcome {
                    result: "ahead".to_owned(),
                    backup_path: None,
                    conflict: Some(info),
                    output: None,
                });
            }
            Some(info) if info.ahead == 0 && info.behind == 0 => {
                // 与上游同步：正常 ff pull（no-op 成功）
                let output = git::run_git(path, &["pull", "--ff-only"])?;
                return Ok(PullOutcome {
                    result: "ok".to_owned(),
                    backup_path: None,
                    conflict: None,
                    output: Some(output),
                });
            }
            Some(info) => {
                // diverged / non-ff：进冲突决策，携带双栏清单
                return resolve_pull_conflict_with_info(path, repo, policy, mode, Some(info));
            }
            None => {}
        }
        let pull_result = git::run_git(path, &["pull", "--ff-only"]);
        match pull_result {
            Ok(output) => Ok(PullOutcome {
                result: "ok".to_owned(),
                backup_path: None,
                conflict: None,
                output: Some(output),
            }),
            Err(VcsError::GitCommand(message)) if is_conflict_message(&message) => {
                resolve_pull_conflict_with_info(path, repo, policy, mode, None)
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

fn dirty_conflict_info(metadata: &git::GitMetadata) -> ConflictInfo {
    ConflictInfo {
        kind: crate::types::ConflictKind::Dirty,
        reason: format!(
            "working tree has local changes: {} staged, {} modified, {} untracked",
            metadata.staged, metadata.modified, metadata.untracked
        ),
        ahead: metadata.ahead,
        behind: metadata.behind,
        merge_base: None,
        merge_base_date: None,
        local_only: Vec::new(),
        upstream_only: Vec::new(),
        equivalent: Vec::new(),
        status_entries: metadata.status_entries.clone(),
        staged: metadata.staged,
        modified: metadata.modified,
        untracked: metadata.untracked,
    }
}

pub fn resolve_pull_conflict(
    path: &Path,
    repo: &RepoRecord,
    action: PullConflictAction,
) -> Result<PullOutcome, VcsError> {
    resolve_pull_conflict_with_info(path, repo, &ResolvedPolicy {
        pull_strategy: "fetch-only".to_owned(),
        conflict_action: action,
        unattended_action: action,
    }, PullMode::Interactive, None)
}

fn resolve_pull_conflict_with_info(
    path: &Path,
    repo: &RepoRecord,
    policy: &ResolvedPolicy,
    mode: PullMode,
    info: Option<ConflictInfo>,
) -> Result<PullOutcome, VcsError> {
    let action = match mode {
        PullMode::Interactive => policy.conflict_action,
        PullMode::Unattended => policy.unattended_action,
    };
    match action {
        PullConflictAction::Stop => Err(VcsError::ConflictNeedsDecision(info.map(Box::new))),
        PullConflictAction::Abort => Ok(PullOutcome {
            result: "aborted".to_owned(),
            backup_path: None,
            conflict: info,
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
                        conflict: info,
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
                conflict: info,
                output: Some(output),
            })
        }
    }
}

fn backup_path(path: &Path) -> PathBuf {
    let timestamp = Utc::now().format("%Y%m%d%H%M%S%.3f");
    let base = format!("{}.bak.{}", path.display(), timestamp);
    let mut candidate = PathBuf::from(&base);
    let mut counter = 1u32;
    while candidate.exists() {
        candidate = PathBuf::from(format!("{base}_{counter}"));
        counter += 1;
        if counter > 100 {
            let fallback = Utc::now().format("%Y%m%d%H%M%S%.6f");
            candidate = PathBuf::from(format!("{}.bak.{}", path.display(), fallback));
            break;
        }
    }
    candidate
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
