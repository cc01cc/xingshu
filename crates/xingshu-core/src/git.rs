use std::path::{Path, PathBuf};
use std::process::Command;

use crate::types::{CommitBrief, ConflictInfo, ConflictKind, StatusEntry, VcsError};

#[derive(Debug, Clone)]
pub struct GitMetadata {
    pub remote_url: Option<String>,
    pub default_branch: Option<String>,
    pub head_commit: Option<String>,
    pub is_bare: bool,
    pub dirty: bool,
    pub ahead_of_upstream: bool,
    pub ahead: u32,
    pub behind: u32,
    pub status_entries: Vec<StatusEntry>,
    pub staged: u32,
    pub modified: u32,
    pub untracked: u32,
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
    let (ahead, behind) = status
        .lines()
        .next()
        .map(parse_status_branch_line)
        .unwrap_or((0, 0));
    let (status_entries, staged, modified, untracked) = parse_status_entries(&status);
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
        dirty: !status_entries.is_empty(),
        ahead_of_upstream: ahead > 0,
        ahead,
        behind,
        staged,
        modified,
        untracked,
        status_entries,
        remotes,
    })
}

fn parse_status_branch_line(line: &str) -> (u32, u32) {
    let mut ahead = 0;
    let mut behind = 0;
    if let Some(marker) = line.find("[ahead ") {
        let rest = &line[marker + 7..];
        if let Some(end) = rest.find(']') {
            ahead = rest[..end].trim().parse().unwrap_or(0);
        }
    }
    if let Some(marker) = line.find("behind ") {
        let rest = &line[marker + 7..];
        if let Some(end) = rest.find(']') {
            behind = rest[..end].trim().parse().unwrap_or(0);
        }
    }
    (ahead, behind)
}

fn parse_status_entries(status: &str) -> (Vec<StatusEntry>, u32, u32, u32) {
    let mut entries = Vec::new();
    let mut staged = 0;
    let mut modified = 0;
    let mut untracked = 0;
    for line in status.lines().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let bytes = line.as_bytes();
        if bytes.len() < 3 {
            continue;
        }
        let index = String::from_utf8_lossy(&bytes[..1]).to_string();
        let worktree = String::from_utf8_lossy(&bytes[1..2]).to_string();
        let path = line[3..].to_string();
        if index == "?" {
            untracked += 1;
        } else if index != " " {
            staged += 1;
        } else {
            modified += 1;
        }
        entries.push(StatusEntry {
            index,
            worktree,
            path,
        });
    }
    (entries, staged, modified, untracked)
}

pub fn divergence(path: &Path) -> Option<ConflictInfo> {
    let branch = run_git(path, &["rev-parse", "--abbrev-ref", "HEAD"]).ok()?;
    let branch = branch.stdout.trim().to_owned();
    if branch.is_empty() || branch == "HEAD" {
        return None;
    }
    let upstream = format!("origin/{branch}");
    run_git(path, &["rev-parse", "--verify", "-q", &upstream]).ok()?;
    let head = run_git(path, &["rev-parse", "HEAD"])
        .ok()?
        .stdout
        .trim()
        .to_owned();
    let upstream_commit = run_git(path, &["rev-parse", &upstream])
        .ok()?
        .stdout
        .trim()
        .to_owned();
    let merge_base = run_git(path, &["merge-base", "HEAD", &upstream])
        .ok()?
        .stdout
        .trim()
        .to_owned();
    let ahead = count_reachable(path, &head, &merge_base);
    let behind = count_reachable(path, &upstream_commit, &merge_base);
    let merge_base_date = commit_date(path, &merge_base);
    let local_only = commit_briefs(path, format!("{head}...{merge_base}"));
    let upstream_only = commit_briefs(path, format!("{upstream}...{merge_base}"));
    let equivalent = cherry_equivalent(path, &merge_base, &upstream);
    Some(ConflictInfo {
        kind: if behind > 0 && ahead > 0 {
            ConflictKind::Diverged
        } else {
            ConflictKind::NonFf
        },
        reason: if ahead > 0 && behind > 0 {
            format!("local diverged from upstream: ahead {ahead}, behind {behind}")
        } else if behind > 0 {
            format!("non-fast-forward: upstream rewritten (behind {behind})")
        } else {
            format!("non-fast-forward (ahead {ahead})")
        },
        ahead,
        behind,
        merge_base: Some(short(&merge_base)),
        merge_base_date,
        local_only,
        upstream_only,
        equivalent,
        status_entries: Vec::new(),
        staged: 0,
        modified: 0,
        untracked: 0,
    })
}

fn count_reachable(path: &Path, from: &str, exclude: &str) -> u32 {
    run_git(path, &["rev-list", "--count", from, &format!("^{exclude}")])
        .ok()
        .and_then(|output| output.stdout.trim().parse().ok())
        .unwrap_or(0)
}

fn short(commit: &str) -> String {
    commit.chars().take(7).collect()
}

fn commit_date(path: &Path, commit: &str) -> Option<String> {
    if commit.is_empty() {
        return None;
    }
    run_git(path, &["log", "-1", "--format=%cs", commit])
        .ok()
        .map(|output| output.stdout.trim().to_owned())
}

fn commit_briefs(path: &Path, range: String) -> Vec<CommitBrief> {
    let output = match run_git(
        path,
        &[
            "log",
            "--max-count=20",
            "--format=%h%x1f%s%x1f%an%x1f%cs",
            &range,
        ],
    ) {
        Ok(output) => output,
        Err(_) => return Vec::new(),
    };
    output
        .stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let parts: Vec<&str> = line.split('\u{1f}').collect();
            CommitBrief {
                short: parts.first().unwrap_or(&"").to_string(),
                summary: parts.get(1).unwrap_or(&"").to_string(),
                author: parts.get(2).unwrap_or(&"").to_string(),
                date: parts.get(3).unwrap_or(&"").to_string(),
            }
        })
        .collect()
}

fn cherry_equivalent(path: &Path, merge_base: &str, upstream: &str) -> Vec<(String, String)> {
    // `git cherry <upstream> <head>`: lists local-side commits (reachable from HEAD,
    // not from upstream). '-' lines are patch-equivalent to some upstream commit;
    // pair each with its upstream counterpart by patch-id so the UI can render
    // "9f03b2c ≡ c1d2e3f（内容相同）".
    let output = match run_git(path, &["cherry", upstream, "HEAD"]) {
        Ok(output) => output,
        Err(_) => return Vec::new(),
    };
    let mut equivalent_locals: Vec<String> = Vec::new();
    for line in output.stdout.lines() {
        let bytes = line.as_bytes();
        if bytes.len() < 2 {
            continue;
        }
        let marker = bytes[0] as char;
        let commit = line[2..].split_whitespace().next().unwrap_or("");
        if marker == '-' {
            equivalent_locals.push(commit.to_string());
        }
    }
    if equivalent_locals.is_empty() {
        return Vec::new();
    }
    // Patch-id map for the upstream side (merge_base..upstream).
    let upstream_ids = side_patch_id_map(path, upstream, merge_base);
    let mut pairs = Vec::new();
    for local_commit in equivalent_locals {
        if let Some(patch_id) = patch_id(path, &local_commit)
            && let Some(upstream_commit) = upstream_ids.get(&patch_id)
        {
            pairs.push((short(&local_commit), short(upstream_commit)));
        }
    }
    pairs
}

fn side_patch_id_map(
    path: &Path,
    include: &str,
    exclude: &str,
) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let log = match run_git(
        path,
        &["log", "--format=%H", include, &format!("^{exclude}")],
    ) {
        Ok(log) => log,
        Err(_) => return map,
    };
    for commit in log.stdout.lines().filter(|line| !line.trim().is_empty()) {
        if let Some(patch_id) = patch_id(path, commit) {
            map.insert(patch_id, commit.to_string());
        }
    }
    map
}

fn patch_id(path: &Path, commit: &str) -> Option<String> {
    let show = run_git(path, &["show", commit]).ok()?;
    let mut child = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["patch-id", "--stable"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .ok()?;
    use std::io::Write as _;
    let mut stdin = child.stdin.take()?;
    stdin.write_all(show.stdout.as_bytes()).ok()?;
    drop(stdin);
    let output = child.wait_with_output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    text.lines()
        .next()
        .and_then(|line| line.split_whitespace().next())
        .map(str::to_string)
        .filter(|value| !value.is_empty())
}

pub fn run_git(path: &Path, args: &[&str]) -> Result<GitCommandOutput, VcsError> {
    run_git_with_program(std::ffi::OsStr::new("git"), path, args)
}

fn run_git_with_program(
    program: &std::ffi::OsStr,
    path: &Path,
    args: &[&str],
) -> Result<GitCommandOutput, VcsError> {
    let output = Command::new(program)
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .map_err(git_start_error)?;
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

pub(crate) fn git_start_error(error: std::io::Error) -> VcsError {
    if error.kind() == std::io::ErrorKind::NotFound {
        VcsError::GitCommand(
            "Git executable not found on PATH; install Git and ensure `git` is available, then restart the terminal".to_owned(),
        )
    } else {
        VcsError::GitCommand(error.to_string())
    }
}

/// Read the remote's advertised heads with `git ls-remote` (PLAN-297 ISS-02B).
///
/// This is strictly read-only: no objects, refs, or worktree state change
/// locally. Requires network access and credentials when the remote is private.
pub fn remote_refs(path: &Path, remote: &str) -> Result<Vec<(String, String)>, VcsError> {
    let output = run_git(path, &["ls-remote", "--heads", remote])?;
    Ok(output
        .stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let sha = parts.next()?;
            let name = parts.next()?;
            Some((name.to_owned(), sha.to_owned()))
        })
        .collect())
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

/// Upper bound for a single-file patch returned by [`diff_text`] (PLAN-0316).
pub const DIFF_MAX_BYTES: usize = 512 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    Diff,
    Binary,
    Untracked,
    NoHead,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffText {
    pub kind: DiffKind,
    pub patch: String,
    pub truncated: bool,
}

/// Read-only working-tree diff for one repository-relative path
/// (staged + unstaged versus HEAD), capped at [`DIFF_MAX_BYTES`].
pub fn diff_text(path: &Path, rel_path: &str) -> Result<DiffText, VcsError> {
    if run_git(path, &["rev-parse", "--verify", "-q", "HEAD"]).is_err() {
        return Ok(DiffText {
            kind: DiffKind::NoHead,
            patch: String::new(),
            truncated: false,
        });
    }
    let tracked = run_git(path, &["ls-files", "--error-unmatch", "--", rel_path]).is_ok();
    if !tracked {
        if !path.join(rel_path).exists() {
            return Err(VcsError::Filesystem(format!("not found: {rel_path}")));
        }
        return Ok(DiffText {
            kind: DiffKind::Untracked,
            patch: String::new(),
            truncated: false,
        });
    }
    let numstat = run_git(path, &["diff", "--numstat", "HEAD", "--", rel_path])?;
    if numstat
        .stdout
        .lines()
        .any(|line| line.starts_with("-\t-\t"))
    {
        return Ok(DiffText {
            kind: DiffKind::Binary,
            patch: String::new(),
            truncated: false,
        });
    }
    let output = run_git(path, &["diff", "HEAD", "--no-color", "--", rel_path])?;
    let (patch, truncated) = cap_diff_text(&output.stdout, DIFF_MAX_BYTES);
    Ok(DiffText {
        kind: DiffKind::Diff,
        patch,
        truncated,
    })
}

fn cap_diff_text(text: &str, limit: usize) -> (String, bool) {
    if text.len() <= limit {
        return (text.to_owned(), false);
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_owned(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_git_executable_has_install_guidance() {
        let temp = tempfile::tempdir().expect("tempdir");
        let missing_git = temp.path().join("missing-git-executable.exe");
        let error = run_git_with_program(missing_git.as_os_str(), temp.path(), &["--version"])
            .expect_err("missing executable must fail");
        let VcsError::GitCommand(message) = error else {
            panic!("unexpected error: {error:?}");
        };
        assert!(
            message.contains("Git executable not found on PATH"),
            "{message}"
        );
        assert!(message.contains("install Git"), "{message}");
    }
}
