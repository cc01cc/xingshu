use std::fs;
use std::path::{Component, Path};

use chrono::Utc;

use crate::db::Database;
use crate::git::{self, GitMetadata};
use crate::progress::ProgressReporter;
use crate::types::{RepoKind, RepoRecord, Root, ScanOptions, VcsError};

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScanReport {
    pub roots_scanned: usize,
    pub repos_found: usize,
    pub broken_repos: usize,
    pub skipped_directories: usize,
    pub nested_repos: usize,
    pub errors: Vec<String>,
}

pub fn scan_roots(
    database: &Database,
    roots: &[Root],
    options: &ScanOptions,
) -> Result<ScanReport, VcsError> {
    scan_roots_with_reporter(database, roots, options, None)
}

pub fn scan_roots_with_reporter(
    database: &Database,
    roots: &[Root],
    options: &ScanOptions,
    reporter: Option<&dyn ProgressReporter>,
) -> Result<ScanReport, VcsError> {
    let mut report = ScanReport::default();
    if let Some(reporter) = reporter {
        reporter.started(None);
    }
    for root in roots {
        let Some(root_id) = root.id else {
            return Err(VcsError::Database(format!(
                "root {} must have a database id before scanning",
                root.path.display()
            )));
        };
        report.roots_scanned += 1;
        scan_directory(
            database,
            root_id,
            &root.path,
            &root.path,
            0,
            options,
            reporter,
            &mut report,
        )?;
    }
    if let Some(reporter) = reporter {
        reporter.finished("completed");
    }
    Ok(report)
}

#[allow(clippy::too_many_arguments)]
fn scan_directory(
    database: &Database,
    root_id: i64,
    root_path: &Path,
    current: &Path,
    depth: usize,
    options: &ScanOptions,
    reporter: Option<&dyn ProgressReporter>,
    report: &mut ScanReport,
) -> Result<(), VcsError> {
    if is_generated_backup(current) {
        report.skipped_directories += 1;
        return Ok(());
    }

    if git::has_git_marker(current) {
        let relative = current.strip_prefix(root_path).unwrap_or(current);
        let metadata = git::inspect(current);
        let (git_metadata, clone_status) = match metadata {
            Ok(value) => (value, "ok".to_owned()),
            Err(error) => {
                report
                    .errors
                    .push(format!("{}: {error}", current.display()));
                (
                    GitMetadata {
                        remote_url: None,
                        default_branch: None,
                        head_commit: None,
                        is_bare: git::is_bare_layout(current),
                        dirty: false,
                        ahead_of_upstream: false,
                        remotes: Vec::new(),
                    },
                    "broken".to_owned(),
                )
            }
        };
        let repo_kind = classify(&git_metadata, options);
        let lock_violation = repo_kind.default_modify_lock() && git_metadata.dirty;
        if clone_status == "broken" {
            report.broken_repos += 1;
        }
        let push_protect_remote = match &repo_kind {
            RepoKind::ThirdParty => Some("origin".to_owned()),
            RepoKind::Fork => Some("upstream".to_owned()),
            RepoKind::ThirdPartyFrozen | RepoKind::Own => None,
        };
        let (org, name) = identify_repo(relative, git_metadata.remote_url.as_deref(), current);
        let repo = RepoRecord {
            id: None,
            root_id,
            org,
            name,
            rel_path: relative.to_path_buf(),
            remote_url: git_metadata.remote_url,
            default_branch: git_metadata.default_branch,
            head_commit: git_metadata.head_commit,
            last_fetch_at: None,
            last_pull_status: None,
            size_bytes: directory_size(current),
            is_bare: git_metadata.is_bare,
            clone_status,
            modify_lock: repo_kind.default_modify_lock(),
            lock_violation,
            repo_kind,
            push_protect_remote,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let repo_id = database.upsert_repo(&repo)?;
        database.set_lock_violation(repo_id, lock_violation)?;
        tracing::debug!(
            root_id,
            repo_id,
            rel_path = %repo.rel_path.display(),
            repo_kind = repo.repo_kind.as_str(),
            "indexed repository"
        );
        report.repos_found += 1;
        if let Some(reporter) = reporter {
            reporter.item_finished(&relative.to_string_lossy(), &repo.clone_status, None);
        }
        if lock_violation {
            report.errors.push(format!(
                "{}: lock_violation (working tree contains local changes)",
                current.display()
            ));
        }
        if !options.include_nested {
            return Ok(());
        }
        report.nested_repos += scan_children(
            database, root_id, root_path, current, depth, options, reporter, report,
        )?;
        return Ok(());
    }

    scan_children(
        database, root_id, root_path, current, depth, options, reporter, report,
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn scan_children(
    database: &Database,
    root_id: i64,
    root_path: &Path,
    current: &Path,
    depth: usize,
    options: &ScanOptions,
    reporter: Option<&dyn ProgressReporter>,
    report: &mut ScanReport,
) -> Result<usize, VcsError> {
    if options.max_depth.is_some_and(|limit| depth >= limit) {
        return Ok(0);
    }
    let entries = fs::read_dir(current)
        .map_err(|error| VcsError::Filesystem(format!("{}: {error}", current.display())))?;
    let mut nested = 0;
    for entry in entries {
        let entry = entry.map_err(|error| VcsError::Filesystem(error.to_string()))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if should_skip_dir(&name, options) || is_generated_backup(&path) {
            report.skipped_directories += 1;
            continue;
        }
        let before = report.repos_found;
        scan_directory(
            database,
            root_id,
            root_path,
            &path,
            depth + 1,
            options,
            reporter,
            report,
        )?;
        if report.repos_found > before {
            nested += report.repos_found - before;
        }
    }
    Ok(nested)
}

fn should_skip_dir(name: &str, options: &ScanOptions) -> bool {
    options
        .skip_dirs
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(name))
}

fn is_generated_backup(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    name.contains(".broken.") || name.contains(".bak.")
}

fn classify(metadata: &GitMetadata, options: &ScanOptions) -> RepoKind {
    if metadata.remotes.iter().any(|remote| remote == "upstream") {
        return RepoKind::Fork;
    }
    let owner_matches = metadata
        .remote_url
        .as_deref()
        .map(remote_owner)
        .is_some_and(|owner| options.my_orgs.iter().any(|item| item == &owner));
    if owner_matches {
        RepoKind::Own
    } else {
        RepoKind::ThirdParty
    }
}

fn identify_repo(relative: &Path, remote_url: Option<&str>, current: &Path) -> (String, String) {
    if let Some(remote) = remote_url {
        let segments = remote_segments(remote);
        if segments.len() >= 2 {
            return (
                segments[segments.len() - 2].to_owned(),
                segments[segments.len() - 1]
                    .trim_end_matches(".git")
                    .to_owned(),
            );
        }
    }
    let mut components = relative
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if components.is_empty() {
        components.push(
            current
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| "root".to_owned()),
        );
    }
    let name = components.pop().unwrap_or_else(|| "unknown".to_owned());
    let org = components.pop().unwrap_or_else(|| "local".to_owned());
    (org, name)
}

fn remote_owner(remote_url: &str) -> String {
    remote_segments(remote_url)
        .into_iter()
        .rev()
        .nth(1)
        .unwrap_or_default()
        .to_owned()
}

fn remote_segments(remote_url: &str) -> Vec<&str> {
    remote_url
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit_once(':')
        .map(|(_, path)| path)
        .unwrap_or(remote_url)
        .split('/')
        .filter(|segment| !segment.is_empty() && !segment.contains("@"))
        .collect()
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
