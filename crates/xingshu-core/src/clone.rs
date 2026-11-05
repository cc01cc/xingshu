use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::db::Database;
use crate::git;
use crate::scanner::build_repo_record;
use crate::types::{RepoRecord, ScanOptions, VcsError};

/// Clone one remote into a new path inside a selected registered root and
/// register the resulting repository without scanning the whole root.
pub fn clone_and_register(
    database: &Database,
    root_id: i64,
    remote: &str,
    destination: &Path,
    my_orgs: &[String],
) -> Result<RepoRecord, VcsError> {
    let root = database
        .find_root_by_id(root_id)?
        .ok_or_else(|| VcsError::Database("registered root was not found".to_owned()))?;
    let root_id = root
        .id
        .ok_or_else(|| VcsError::Database("registered root has no database id".to_owned()))?;
    let relative = normalize_destination(destination)?;
    validate_remote(remote)?;

    let target_key = relative_path_key(&relative)
        .ok_or_else(|| VcsError::Filesystem("invalid relative destination".to_owned()))?;
    if database.list_repos()?.iter().any(|repo| {
        repo.root_id == root_id
            && relative_path_key(&repo.rel_path).is_some_and(|key| key == target_key)
    }) {
        return Err(VcsError::Database(
            "repository is already registered at the destination; repair the stale registration before cloning"
                .to_owned(),
        ));
    }

    let canonical_root = fs::canonicalize(&root.path)
        .map_err(|_| VcsError::Filesystem("registered root is not accessible".to_owned()))?;
    if !canonical_root.is_dir() {
        return Err(VcsError::Filesystem(
            "registered root is not a directory".to_owned(),
        ));
    }
    let parent_relative = relative.parent().unwrap_or_else(|| Path::new(""));
    let canonical_parent = canonical_destination_parent(&canonical_root, parent_relative)?;
    let file_name = relative
        .file_name()
        .ok_or_else(|| VcsError::Filesystem("destination must name a directory".to_owned()))?;
    let target = canonical_parent.join(file_name);
    let git_target = git_compatible_path(&target)?;
    let git_parent = git_target
        .parent()
        .ok_or_else(|| VcsError::Filesystem("destination must name a directory".to_owned()))?;
    let canonical_git_parent = fs::canonicalize(git_parent)
        .map_err(|_| VcsError::Filesystem("destination parent is not accessible".to_owned()))?;
    if !path_is_within(&canonical_parent, &canonical_git_parent)
        || !path_is_within(&canonical_git_parent, &canonical_parent)
    {
        return Err(VcsError::PathOutsideRoot);
    }
    reject_existing_destination(&target)?;
    reject_overlapping_root(database, root_id, &target)?;

    fs::create_dir(&target).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            VcsError::Filesystem("destination already exists".to_owned())
        } else {
            VcsError::Filesystem("could not reserve clone destination".to_owned())
        }
    })?;

    let canonical_target_for_git = fs::canonicalize(&target).map_err(|_| {
        VcsError::Filesystem(format!(
            "reserved clone destination is no longer accessible; {}",
            destination_state(&target, &relative)
        ))
    })?;
    let canonical_git_target = fs::canonicalize(&git_target).map_err(|_| {
        VcsError::Filesystem(format!(
            "Git destination path is not accessible; {}",
            destination_state(&target, &relative)
        ))
    })?;
    if !path_is_within(&canonical_target_for_git, &canonical_git_target)
        || !path_is_within(&canonical_git_target, &canonical_target_for_git)
    {
        return Err(VcsError::Filesystem(format!(
            "Git destination does not resolve to the reserved clone destination; {}",
            destination_state(&target, &relative)
        )));
    }

    let output = Command::new("git")
        .arg("clone")
        .arg("--")
        .arg(remote)
        .arg(&git_target)
        .output()
        .map_err(|error| {
            let start_error = crate::git::git_start_error(error);
            match start_error {
                VcsError::GitCommand(message) => VcsError::GitCommand(format!(
                    "{message}; {}",
                    destination_state(&target, &relative)
                )),
                other => other,
            }
        })?;
    if !output.status.success() {
        let status = output
            .status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "terminated".to_owned());
        return Err(VcsError::GitCommand(format!(
            "Git clone failed (exit {status}); details suppressed; {}",
            destination_state(&target, &relative)
        )));
    }

    let current_root = database
        .find_root_by_id(root_id)
        .map_err(|_| {
            VcsError::Database(format!(
                "could not recheck selected root; {}",
                destination_state(&target, &relative)
            ))
        })?
        .ok_or_else(|| {
            VcsError::Database(format!(
                "selected root was removed during clone; {}",
                destination_state(&target, &relative)
            ))
        })?;
    let current_root_path = fs::canonicalize(&current_root.path).map_err(|_| {
        VcsError::Filesystem(format!(
            "selected root is no longer accessible; {}",
            destination_state(&target, &relative)
        ))
    })?;
    let canonical_target = fs::canonicalize(&target).map_err(|_| {
        VcsError::Filesystem(format!(
            "clone destination changed during operation; {}",
            destination_state(&target, &relative)
        ))
    })?;
    if !path_is_within(&current_root_path, &canonical_target) {
        return Err(VcsError::Filesystem(format!(
            "clone destination is outside the selected root; {}",
            destination_state(&target, &relative)
        )));
    }
    if let Err(error) = reject_overlapping_root(database, root_id, &target) {
        return Err(VcsError::Filesystem(format!(
            "{error}; {}",
            destination_state(&target, &relative)
        )));
    }
    let current_repos = database.list_repos().map_err(|_| {
        VcsError::Database(format!(
            "could not recheck repository registration; {}",
            destination_state(&target, &relative)
        ))
    })?;
    if current_repos.iter().any(|repo| {
        repo.root_id == root_id
            && relative_path_key(&repo.rel_path).is_some_and(|key| key == target_key)
    }) {
        return Err(VcsError::Database(format!(
            "repository location was registered during clone; {}",
            destination_state(&target, &relative)
        )));
    }

    if !git::is_bare_layout(&target)
        && git::run_git(&git_target, &["rev-parse", "--verify", "-q", "HEAD"]).is_err()
    {
        return Err(VcsError::GitCommand(format!(
            "remote has no readable HEAD; {}",
            destination_state(&target, &relative)
        )));
    }
    let metadata = git::inspect(&git_target).map_err(|_| {
        VcsError::GitCommand(format!(
            "cloned repository metadata could not be read; {}",
            destination_state(&target, &relative)
        ))
    })?;
    let options = ScanOptions {
        my_orgs: my_orgs.to_vec(),
        ..ScanOptions::default()
    };
    let mut repo = build_repo_record(
        root_id,
        &relative,
        &target,
        metadata,
        &options,
        "ok".to_owned(),
    );
    let repo_id = database.insert_repo(&repo).map_err(|_| {
        VcsError::Database(format!(
            "clone completed but repository registration failed; {}",
            destination_state(&target, &relative)
        ))
    })?;
    repo.id = Some(repo_id);
    Ok(repo)
}

#[cfg(windows)]
fn git_compatible_path(path: &Path) -> Result<PathBuf, VcsError> {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    const SLASH: u16 = b'/' as u16;
    const BACKSLASH: u16 = b'\\' as u16;
    let raw: Vec<u16> = path.as_os_str().encode_wide().collect();
    let verbatim_prefix: Vec<u16> = "\\\\?\\".encode_utf16().collect();
    let verbatim_unc_prefix: Vec<u16> = "\\\\?\\UNC\\".encode_utf16().collect();

    let normal = if raw.starts_with(&verbatim_unc_prefix) {
        let mut normal = vec![BACKSLASH, BACKSLASH];
        normal.extend_from_slice(&raw[verbatim_unc_prefix.len()..]);
        normal
    } else if raw.starts_with(&verbatim_prefix) {
        let remainder = &raw[verbatim_prefix.len()..];
        if !is_windows_drive_absolute(remainder) {
            return Err(VcsError::Filesystem(
                "destination path uses an unsupported Windows device or volume form".to_owned(),
            ));
        }
        remainder.to_vec()
    } else if is_windows_drive_absolute(&raw) || is_normal_unc_path(&raw) {
        raw
    } else {
        return Err(VcsError::Filesystem(
            "destination path cannot be represented as a Windows drive or UNC path".to_owned(),
        ));
    };

    let aliases_win32_component = normal
        .split(|unit| *unit == SLASH || *unit == BACKSLASH)
        .any(|component| {
            component
                .last()
                .is_some_and(|unit| *unit == b'.' as u16 || *unit == b' ' as u16)
        });
    if aliases_win32_component {
        return Err(VcsError::Filesystem(
            "destination path contains a component Git would normalize on Windows".to_owned(),
        ));
    }

    Ok(PathBuf::from(OsString::from_wide(&normal)))
}

#[cfg(windows)]
fn is_windows_drive_absolute(path: &[u16]) -> bool {
    path.len() >= 3
        && ((path[0] >= b'A' as u16 && path[0] <= b'Z' as u16)
            || (path[0] >= b'a' as u16 && path[0] <= b'z' as u16))
        && path[1] == b':' as u16
        && (path[2] == b'\\' as u16 || path[2] == b'/' as u16)
}

#[cfg(windows)]
fn is_normal_unc_path(path: &[u16]) -> bool {
    if path.len() < 5
        || path[0] != b'\\' as u16
        || path[1] != b'\\' as u16
        || path[2] == b'?' as u16
        || path[2] == b'.' as u16
    {
        return false;
    }

    let mut components = path[2..]
        .split(|unit| *unit == b'\\' as u16 || *unit == b'/' as u16)
        .filter(|component| !component.is_empty());
    components.next().is_some() && components.next().is_some()
}

#[cfg(not(windows))]
fn git_compatible_path(path: &Path) -> Result<PathBuf, VcsError> {
    Ok(path.to_path_buf())
}

fn normalize_destination(destination: &Path) -> Result<PathBuf, VcsError> {
    if destination.as_os_str().is_empty() || destination.is_absolute() {
        return Err(VcsError::PathOutsideRoot);
    }

    let mut normalized = PathBuf::new();
    for component in destination.components() {
        match component {
            Component::Normal(part) => {
                if part.to_string_lossy().chars().any(char::is_control) {
                    return Err(VcsError::Filesystem(
                        "destination contains control characters".to_owned(),
                    ));
                }
                normalized.push(part);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(VcsError::PathOutsideRoot);
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(VcsError::PathOutsideRoot);
    }
    Ok(normalized)
}

fn canonical_destination_parent(root: &Path, relative_parent: &Path) -> Result<PathBuf, VcsError> {
    let mut current = root.to_path_buf();
    for component in relative_parent.components() {
        let Component::Normal(part) = component else {
            return Err(VcsError::PathOutsideRoot);
        };
        current.push(part);
        let metadata = fs::symlink_metadata(&current).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                VcsError::Filesystem("destination parent must already exist".to_owned())
            } else {
                VcsError::Filesystem("destination parent is not accessible".to_owned())
            }
        })?;
        if is_reparse_point(&metadata) {
            return Err(VcsError::Filesystem(
                "destination parent cannot contain symlinks or junctions".to_owned(),
            ));
        }
        if !metadata.is_dir() {
            return Err(VcsError::Filesystem(
                "destination parent is not a directory".to_owned(),
            ));
        }
    }
    let canonical = fs::canonicalize(&current)
        .map_err(|_| VcsError::Filesystem("destination parent is not accessible".to_owned()))?;
    if !path_is_within(root, &canonical) {
        return Err(VcsError::PathOutsideRoot);
    }
    Ok(canonical)
}

fn reject_existing_destination(target: &Path) -> Result<(), VcsError> {
    match fs::symlink_metadata(target) {
        Ok(_) => Err(VcsError::Filesystem(
            "destination already exists".to_owned(),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(VcsError::Filesystem(
            "could not inspect clone destination".to_owned(),
        )),
    }
}

fn reject_overlapping_root(
    database: &Database,
    selected_root_id: i64,
    target: &Path,
) -> Result<(), VcsError> {
    for root in database.list_roots()? {
        if root.id == Some(selected_root_id) {
            continue;
        }
        let Ok(other_root) = fs::canonicalize(&root.path) else {
            continue;
        };
        if path_is_within(&other_root, target) {
            return Err(VcsError::Filesystem(
                "destination is inside another registered root".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_remote(remote: &str) -> Result<(), VcsError> {
    if remote.is_empty()
        || remote.trim() != remote
        || remote.chars().any(char::is_control)
        || remote.contains('?')
        || remote.contains('#')
    {
        return Err(VcsError::GitCommand(
            "remote URL is invalid or contains unsupported authentication data".to_owned(),
        ));
    }

    if let Some((scheme, remainder)) = remote.split_once("://") {
        let authority_end = remainder.find('/').unwrap_or(remainder.len());
        let authority = &remainder[..authority_end];
        let path = &remainder[authority_end..];
        if authority.contains('%') {
            return Err(invalid_remote());
        }
        match scheme.to_ascii_lowercase().as_str() {
            "http" | "https" => {
                if authority.is_empty() || authority.contains('@') || path.is_empty() {
                    return Err(invalid_remote());
                }
            }
            "ssh" => {
                if !valid_ssh_authority(authority) || path.is_empty() {
                    return Err(invalid_remote());
                }
            }
            "file" => {
                if authority.contains('@') || path.is_empty() {
                    return Err(invalid_remote());
                }
            }
            _ => return Err(invalid_remote()),
        }
        return Ok(());
    }

    if let Some((authority, path)) = remote.rsplit_once(':') {
        if authority.contains('/')
            || authority.contains('%')
            || path.is_empty()
            || (authority.len() == 1 && authority.as_bytes()[0].is_ascii_alphabetic())
            || !valid_ssh_authority(authority)
        {
            return Err(invalid_remote());
        }
        return Ok(());
    }

    Err(invalid_remote())
}

fn valid_ssh_authority(authority: &str) -> bool {
    if let Some((username, host)) = authority.split_once('@') {
        username == "git" && !host.is_empty() && !host.contains('@')
    } else {
        !authority.is_empty()
    }
}

fn invalid_remote() -> VcsError {
    VcsError::GitCommand(
        "remote URL is invalid or contains unsupported authentication data".to_owned(),
    )
}

fn relative_path_key(path: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                let value = part.to_string_lossy().replace('\\', "/");
                #[cfg(windows)]
                let value = value.to_lowercase();
                parts.push(value);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn destination_state(target: &Path, relative: &Path) -> String {
    match fs::symlink_metadata(target) {
        Ok(_) => format!(
            "destination remains and may be partial: {}",
            display_relative(relative)
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => format!(
            "destination no longer exists: {}",
            display_relative(relative)
        ),
        Err(_) => format!(
            "destination state is unknown: {}",
            display_relative(relative)
        ),
    }
}

fn display_relative(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn path_is_within(root: &Path, candidate: &Path) -> bool {
    #[cfg(windows)]
    {
        let root = root.to_string_lossy().replace('/', "\\").to_lowercase();
        let candidate = candidate
            .to_string_lossy()
            .replace('/', "\\")
            .to_lowercase();
        let root = root.trim_end_matches('\\');
        candidate == root
            || candidate
                .strip_prefix(root)
                .is_some_and(|remainder| remainder.starts_with('\\'))
    }
    #[cfg(not(windows))]
    {
        candidate.starts_with(root)
    }
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(all(test, windows))]
mod windows_path_tests {
    use super::git_compatible_path;
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::PathBuf;

    #[test]
    fn rejects_components_git_would_normalize() {
        for suffix in [".", " "] {
            let path = PathBuf::from(format!(r"\\?\C:\root\outside{suffix}"));
            assert!(git_compatible_path(&path).is_err());
        }
    }

    #[test]
    fn strips_drive_prefix_without_lossy_unicode_conversion() {
        let path = PathBuf::from(r"\\?\C:\root\café");
        let converted = git_compatible_path(&path).unwrap();
        assert_eq!(converted, PathBuf::from(r"C:\root\café"));
    }

    #[test]
    fn preserves_non_unicode_windows_path_components() {
        let mut verbatim = r"\\?\C:\root\".encode_utf16().collect::<Vec<_>>();
        verbatim.push(0xd800);
        let path = PathBuf::from(OsString::from_wide(&verbatim));

        let mut expected = r"C:\root\".encode_utf16().collect::<Vec<_>>();
        expected.push(0xd800);
        let converted = git_compatible_path(&path).unwrap();
        assert_eq!(
            converted.as_os_str().encode_wide().collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn strips_unc_prefix_without_losing_share_path() {
        let path = PathBuf::from(r"\\?\UNC\server\share\repo");
        let converted = git_compatible_path(&path).unwrap();
        assert_eq!(converted, PathBuf::from(r"\\server\share\repo"));
    }

    #[test]
    fn rejects_unsupported_volume_namespace() {
        let path = PathBuf::from(r"\\?\Volume{test}\repo");
        assert!(git_compatible_path(&path).is_err());
    }
}
