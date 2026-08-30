use std::path::{Path, PathBuf};
use std::process::Command;

use crate::types::VcsError;

#[derive(Debug, Clone)]
pub struct GitMetadata {
    pub remote_url: Option<String>,
    pub default_branch: Option<String>,
    pub head_commit: Option<String>,
    pub is_bare: bool,
    pub dirty: bool,
    pub ahead_of_upstream: bool,
    pub remotes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GitCommandOutput {
    pub stdout: String,
    pub stderr: String,
}

pub fn has_git_marker(path: &Path) -> bool {
    path.join(".git").exists() || is_bare_layout(path)
}

pub fn is_bare_layout(path: &Path) -> bool {
    path.join("HEAD").is_file() && path.join("objects").is_dir() && path.join("refs").is_dir()
}

pub fn inspect(path: &Path) -> Result<GitMetadata, VcsError> {
    let bare = is_bare_layout(path)
        || run_git(path, &["rev-parse", "--is-bare-repository"])
            .map(|output| output.stdout.trim() == "true")
            .unwrap_or(false);
    let head_commit = run_git(path, &["rev-parse", "HEAD"])
        .ok()
        .map(|output| output.stdout.trim().to_owned())
        .filter(|value| !value.is_empty());
    let default_branch = run_git(path, &["symbolic-ref", "--short", "HEAD"])
        .ok()
        .map(|output| output.stdout.trim().to_owned())
        .filter(|value| !value.is_empty());
    let remote_url = run_git(path, &["remote", "get-url", "origin"])
        .ok()
        .map(|output| sanitize_remote_url(output.stdout.trim()))
        .filter(|value| !value.is_empty());
    let remotes = run_git(path, &["remote"])
        .map(|output| {
            output
                .stdout
                .lines()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let status = if bare {
        String::new()
    } else {
        run_git(
            path,
            &[
                "status",
                "--porcelain",
                "--branch",
                "--untracked-files=normal",
            ],
        )
        .map(|output| output.stdout)
        .unwrap_or_default()
    };
    if head_commit.is_none() && !bare {
        return Err(VcsError::GitCommand(format!(
            "{} is not a readable Git repository",
            path.display()
        )));
    }

    Ok(GitMetadata {
        remote_url,
        default_branch,
        head_commit,
        is_bare: bare,
        dirty: status.lines().skip(1).any(|line| !line.trim().is_empty()),
        ahead_of_upstream: status
            .lines()
            .next()
            .is_some_and(|line| line.contains("ahead ")),
        remotes,
    })
}

pub fn run_git(path: &Path, args: &[&str]) -> Result<GitCommandOutput, VcsError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .map_err(|error| VcsError::GitCommand(error.to_string()))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(VcsError::GitCommand(if stderr.trim().is_empty() {
            format!("git {:?} exited with {}", args, output.status)
        } else {
            stderr.trim().to_owned()
        }));
    }
    Ok(GitCommandOutput { stdout, stderr })
}

pub fn sanitize_remote_url(value: &str) -> String {
    if let Some(scheme_end) = value.find("://") {
        let authority_start = scheme_end + 3;
        if let Some(authority_end_offset) = value[authority_start..].find('/') {
            let authority_end = authority_start + authority_end_offset;
            let authority = &value[authority_start..authority_end];
            if let Some(at) = authority.rfind('@') {
                return format!(
                    "{}{}{}",
                    &value[..authority_start],
                    &authority[at + 1..],
                    &value[authority_end..]
                );
            }
        }
    }
    value.to_owned()
}

pub fn repo_path(root: &Path, relative: &Path) -> PathBuf {
    root.join(relative)
}
