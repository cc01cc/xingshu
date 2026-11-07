use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub const ENV_NAME: &str = "XINGSHU_ENV";
pub const ENV_PROD: &str = "prod";
pub const ENV_DEV: &str = "dev";
pub const DB_ENV_VAR: &str = "XINGSHU_DB";
pub const DEV_DEFAULT_DB: &str = ".staging/dev-001/xingshu-dev.db";
pub const PROD_DEFAULT_DB: &str = "xingshu-prod.db";
pub const STAGING_DIR: &str = ".staging";

pub fn current_env() -> String {
    std::env::var(ENV_NAME).unwrap_or_else(|_| ENV_DEV.to_owned())
}

pub fn is_prod() -> bool {
    current_env() == ENV_PROD
}

pub fn resolve_db_path(cli_db: Option<PathBuf>) -> Result<PathBuf, String> {
    resolve_db_path_with(
        cli_db,
        std::env::var_os(DB_ENV_VAR),
        std::env::var(ENV_NAME).ok(),
    )
}

pub fn resolve_db_path_with(
    cli_db: Option<PathBuf>,
    env_db: Option<OsString>,
    env_name: Option<String>,
) -> Result<PathBuf, String> {
    if let Some(path) = cli_db {
        return Ok(path);
    }
    if let Some(value) = env_db {
        return Ok(PathBuf::from(value));
    }
    if env_name.as_deref() == Some(ENV_PROD) {
        Ok(PathBuf::from(PROD_DEFAULT_DB))
    } else {
        Ok(PathBuf::from(DEV_DEFAULT_DB))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliEnvironment {
    Dev,
    Prod,
}

impl CliEnvironment {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dev => ENV_DEV,
            Self::Prod => ENV_PROD,
        }
    }

    pub const fn is_prod(self) -> bool {
        matches!(self, Self::Prod)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliDbConfig {
    pub environment: CliEnvironment,
    pub path: PathBuf,
}

pub fn resolve_cli_environment(
    env_name: Option<&str>,
    release_build: bool,
) -> Result<CliEnvironment, String> {
    match env_name {
        Some(ENV_DEV) => Ok(CliEnvironment::Dev),
        Some(ENV_PROD) => Ok(CliEnvironment::Prod),
        Some(value) => Err(format!(
            "invalid {ENV_NAME} value {value:?}; expected {ENV_DEV:?} or {ENV_PROD:?}"
        )),
        None if release_build => Ok(CliEnvironment::Prod),
        None => Ok(CliEnvironment::Dev),
    }
}

/// Resolve a CLI database path without changing the legacy server resolver above.
/// `release_build` is compile-time metadata supplied by the CLI binary.
pub fn resolve_cli_db_path_with(
    cli_db: Option<PathBuf>,
    env_db: Option<OsString>,
    env_name: Option<String>,
    release_build: bool,
    cwd: &Path,
    user_data_root: &Path,
) -> Result<CliDbConfig, String> {
    let environment = resolve_cli_environment(env_name.as_deref(), release_build)?;
    Ok(resolve_cli_db_path_for_environment_with(
        cli_db,
        env_db,
        environment,
        release_build,
        cwd,
        user_data_root,
    ))
}

pub fn resolve_cli_db_path_for_environment_with(
    cli_db: Option<PathBuf>,
    env_db: Option<OsString>,
    environment: CliEnvironment,
    release_build: bool,
    cwd: &Path,
    user_data_root: &Path,
) -> CliDbConfig {
    let path = if let Some(path) = cli_db {
        path
    } else if let Some(path) = env_db {
        PathBuf::from(path)
    } else {
        match (release_build, environment) {
            (false, CliEnvironment::Dev) => PathBuf::from(DEV_DEFAULT_DB),
            (false, CliEnvironment::Prod) | (true, CliEnvironment::Prod) => {
                user_data_root.join(PROD_DEFAULT_DB)
            }
            (true, CliEnvironment::Dev) => user_data_root.join(ENV_DEV).join("xingshu-dev.db"),
        }
    };

    let path = if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    };

    CliDbConfig { environment, path }
}

/// Platform-specific per-user data root used by the installed CLI.
pub fn cli_user_data_root() -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        let root = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| {
                "LOCALAPPDATA is required to locate the Xingshu data directory".to_owned()
            })?;
        validate_user_data_root(root, "LOCALAPPDATA").map(|root| root.join("Xingshu"))
    }

    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is required to locate the Xingshu data directory".to_owned())?;
        validate_user_data_root(home, "HOME")
            .map(|home| home.join("Library/Application Support/Xingshu"))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(xdg_data_home) = std::env::var_os("XDG_DATA_HOME") {
            return validate_user_data_root(PathBuf::from(xdg_data_home), "XDG_DATA_HOME")
                .map(|root| root.join("xingshu"));
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "HOME is required to locate the Xingshu data directory".to_owned())?;
        validate_user_data_root(home, "HOME").map(|home| home.join(".local/share/xingshu"))
    }

    #[cfg(not(any(windows, unix)))]
    {
        Err("unsupported platform for the Xingshu user data directory".to_owned())
    }
}

fn validate_user_data_root(path: PathBuf, variable: &str) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return Err(format!(
            "{variable} must contain an absolute path to locate the Xingshu data directory"
        ));
    }
    Ok(path)
}

pub fn check_db_allowed(path: &Path) -> Result<(), String> {
    check_db_allowed_with(path, is_prod())
}

pub fn check_db_allowed_with(path: &Path, prod: bool) -> Result<(), String> {
    check_db_allowed_under(path, prod, &staging_root())
}

fn check_db_allowed_under(path: &Path, prod: bool, staging: &Path) -> Result<(), String> {
    if !prod {
        return Ok(());
    }
    let verdict = db_path_verdict(path, staging);
    // `db_path_error_code` is an internal identifier (logs and unit tests only); it is never
    // written into a user-visible message.
    tracing::debug!(
        code = db_path_error_code(&verdict).unwrap_or("none"),
        path = %path.display(),
        "classified database path"
    );
    match verdict {
        PathVerdict::Inside => Err(format!(
            "refusing to open staging database {} with {}=prod; unset {} or use a dev run",
            path.display(),
            ENV_NAME,
            DB_ENV_VAR
        )),
        PathVerdict::Outside => Ok(()),
        PathVerdict::Undetermined { reason } => Err(format!(
            "cannot classify --db path {} as production: {}; use a path whose parent directory \
             exists, or unset {} and run with {}=dev",
            path.display(),
            reason,
            DB_ENV_VAR,
            ENV_NAME
        )),
    }
}

pub fn staging_root() -> PathBuf {
    to_absolute(Path::new(STAGING_DIR))
}

/// Three-state classification of a database path against the staging directory.
///
/// `Undetermined` means the path could not be resolved at all (illegal characters, an
/// unresolvable symlink/junction, a non-directory component, an unreadable parent). It must
/// never be collapsed into "outside staging": the security check stays fail-closed on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathVerdict {
    Inside,
    Outside,
    Undetermined { reason: String },
}

/// Resolve existing path components before comparing. This catches database paths that enter
/// `.staging` through a symlink while still supporting a database file that has not been created.
pub fn db_path_verdict(path: &Path, dir: &Path) -> PathVerdict {
    let resolved_path = match canonicalize_with_missing_tail(path) {
        Ok(path) => path,
        Err(reason) => return PathVerdict::Undetermined { reason },
    };
    let resolved_dir = match canonicalize_with_missing_tail(dir) {
        Ok(dir) => dir,
        Err(reason) => return PathVerdict::Undetermined { reason },
    };
    if path_starts_with(&resolved_path, &resolved_dir) {
        PathVerdict::Inside
    } else {
        PathVerdict::Outside
    }
}

/// Internal error-code identifier for a verdict (logs and unit tests only).
///
/// The returned names are **not** user-visible message prefixes; user-facing text is the
/// wording produced in `check_db_allowed_under`.
pub fn db_path_error_code(verdict: &PathVerdict) -> Option<&'static str> {
    match verdict {
        PathVerdict::Inside => Some("db_path_in_staging"),
        PathVerdict::Outside => None,
        PathVerdict::Undetermined { .. } => Some("db_path_unresolvable"),
    }
}

/// Coarse boolean view of [`db_path_verdict`].
///
/// Only `Inside` is `true`; `Undetermined` is `false` here, so security-relevant call sites must
/// use `db_path_verdict`/`check_db_allowed_under` (which keep `Undetermined` rejected).
pub fn path_is_under(path: &Path, dir: &Path) -> bool {
    db_path_verdict(path, dir) == PathVerdict::Inside
}

fn canonicalize_with_missing_tail(path: &Path) -> Result<PathBuf, String> {
    let absolute = to_absolute(path);
    let mut existing = absolute.clone();
    let mut suffix = Vec::new();

    loop {
        match std::fs::canonicalize(&existing) {
            Ok(mut resolved) => {
                if !suffix.is_empty() && !resolved.is_dir() {
                    return Err(format!(
                        "{} is not a directory in database path {}",
                        display_path(&resolved),
                        display_path(&absolute)
                    ));
                }
                for component in suffix.iter().rev() {
                    resolved.push(component);
                }
                return Ok(resolved);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match std::fs::symlink_metadata(&existing) {
                    Ok(_) => {
                        return Err(format!(
                            "could not resolve existing symlink in database path {}",
                            absolute.display()
                        ));
                    }
                    Err(metadata_error)
                        if metadata_error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(metadata_error) => return Err(metadata_error.to_string()),
                }
                let name = existing.file_name().ok_or_else(|| {
                    format!("could not resolve database path {}", absolute.display())
                })?;
                suffix.push(name.to_os_string());
                if !existing.pop() {
                    return Err(format!(
                        "could not resolve database path {}",
                        absolute.display()
                    ));
                }
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

/// Render a resolved path without the Windows `\\?\` verbatim prefix that `canonicalize` adds,
/// so OS reasons in user-facing messages stay readable.
fn display_path(path: &Path) -> String {
    let text = path.display().to_string();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
}

fn path_starts_with(path: &Path, dir: &Path) -> bool {
    #[cfg(windows)]
    {
        let mut path_components = path.components();
        for dir_component in dir.components() {
            let Some(path_component) = path_components.next() else {
                return false;
            };
            if !path_component
                .as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(&dir_component.as_os_str().to_string_lossy())
            {
                return false;
            }
        }
        true
    }
    #[cfg(not(windows))]
    {
        path.starts_with(dir)
    }
}

fn to_absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db(value: &str) -> Option<OsString> {
        Some(OsString::from(value))
    }

    fn cli_test_paths() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::TempDir::new().expect("tempdir");
        let cwd = temp.path().join("checkout");
        let user_data = temp.path().join("user-data");
        (temp, cwd, user_data)
    }

    #[test]
    fn db_resolution_matrix() {
        assert_eq!(
            resolve_db_path_with(None, None, None).unwrap(),
            PathBuf::from(DEV_DEFAULT_DB)
        );
        assert_eq!(
            resolve_db_path_with(None, db(".staging/dev-002/xingshu-dev.db"), None).unwrap(),
            PathBuf::from(".staging/dev-002/xingshu-dev.db")
        );
        assert_eq!(
            resolve_db_path_with(None, None, Some("prod".to_owned())).unwrap(),
            PathBuf::from(PROD_DEFAULT_DB)
        );
        assert_eq!(
            resolve_db_path_with(
                Some(PathBuf::from("explicit.db")),
                db(".staging/dev-002/xingshu-dev.db"),
                Some("prod".to_owned())
            )
            .unwrap(),
            PathBuf::from("explicit.db")
        );
    }

    #[test]
    fn cli_defaults_are_separated_by_build_channel() {
        let (_temp, cwd, user_data) = cli_test_paths();

        let source = resolve_cli_db_path_with(None, None, None, false, &cwd, &user_data).unwrap();
        assert_eq!(source.environment, CliEnvironment::Dev);
        assert_eq!(source.path, cwd.join(".staging/dev-001/xingshu-dev.db"));

        let release = resolve_cli_db_path_with(None, None, None, true, &cwd, &user_data).unwrap();
        assert_eq!(release.environment, CliEnvironment::Prod);
        assert_eq!(release.path, user_data.join(PROD_DEFAULT_DB));
    }

    #[test]
    fn cli_environment_override_and_db_precedence_are_explicit() {
        let (_temp, cwd, user_data) = cli_test_paths();

        let release_dev =
            resolve_cli_db_path_with(None, None, Some(ENV_DEV.to_owned()), true, &cwd, &user_data)
                .unwrap();
        assert_eq!(release_dev.environment, CliEnvironment::Dev);
        assert_eq!(release_dev.path, user_data.join("dev/xingshu-dev.db"));

        let env_db = resolve_cli_db_path_with(
            None,
            Some(OsString::from("custom/env.db")),
            Some(ENV_PROD.to_owned()),
            true,
            &cwd,
            &user_data,
        )
        .unwrap();
        assert_eq!(env_db.environment, CliEnvironment::Prod);
        assert_eq!(env_db.path, cwd.join("custom/env.db"));

        let cli_db = resolve_cli_db_path_with(
            Some(PathBuf::from("custom/cli.db")),
            Some(OsString::from("custom/env.db")),
            Some(ENV_PROD.to_owned()),
            true,
            &cwd,
            &user_data,
        )
        .unwrap();
        assert_eq!(cli_db.path, cwd.join("custom/cli.db"));
    }

    #[test]
    fn cli_rejects_unknown_environment_and_relative_data_root() {
        let (_temp, cwd, user_data) = cli_test_paths();
        let error = resolve_cli_db_path_with(
            None,
            None,
            Some("production".to_owned()),
            true,
            &cwd,
            &user_data,
        )
        .unwrap_err();
        assert!(error.contains("invalid XINGSHU_ENV"));

        assert!(validate_user_data_root(PathBuf::from("relative"), "XDG_DATA_HOME").is_err());
    }

    #[test]
    fn server_resolver_keeps_legacy_default_behavior() {
        assert_eq!(
            resolve_db_path_with(None, None, None).unwrap(),
            PathBuf::from(DEV_DEFAULT_DB)
        );
        assert_eq!(
            resolve_db_path_with(None, None, Some(ENV_PROD.to_owned())).unwrap(),
            PathBuf::from(PROD_DEFAULT_DB)
        );
    }

    #[test]
    fn prod_refuses_staging_db() {
        let staging_db = staging_root().join("dev-002/xingshu-dev.db");
        assert!(check_db_allowed_with(&staging_db, true).is_err());
        assert!(check_db_allowed_with(Path::new("xingshu-prod.db"), true).is_ok());
        assert!(check_db_allowed_with(&staging_db, false).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn prod_refuses_staging_database_reached_through_symlink() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("tempdir");
        let staging = temp.path().join(".staging");
        std::fs::create_dir_all(&staging).expect("staging dir");
        let alias = temp.path().join("outside-alias");
        symlink(&staging, &alias).expect("directory symlink");

        assert!(check_db_allowed_under(&alias.join("dev-003/xingshu.db"), true, &staging).is_err());
        assert!(check_db_allowed_under(&temp.path().join("prod.db"), true, &staging).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn prod_refuses_staging_database_reached_through_junction() {
        use std::process::Command;

        let temp = tempfile::tempdir().expect("tempdir");
        let staging = temp.path().join(".staging");
        std::fs::create_dir_all(&staging).expect("staging dir");
        let alias = temp.path().join("outside-alias");
        let junction_script = temp.path().join("create-junction.ps1");
        std::fs::write(
            &junction_script,
            "param([string]$Link, [string]$Target)\n$ErrorActionPreference = 'Stop'\nNew-Item -ItemType Junction -Path $Link -Target $Target | Out-Null\n",
        )
        .expect("write junction helper");
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&junction_script)
            .arg(&alias)
            .arg(&staging)
            .output()
            .expect("create synthetic directory junction");
        assert!(
            output.status.success(),
            "junction creation failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        assert!(check_db_allowed_under(&alias.join("dev-003/xingshu.db"), true, &staging).is_err());
        assert!(check_db_allowed_under(&temp.path().join("prod.db"), true, &staging).is_ok());
    }

    /// A database path whose parent component is a regular file: resolution fails with a
    /// non-`NotFound` error, so the classifier cannot decide whether the path is inside staging.
    fn unresolvable_db_path(temp: &tempfile::TempDir) -> PathBuf {
        let blocker = temp.path().join("blocker.txt");
        std::fs::write(&blocker, b"not a directory").expect("blocker file");
        blocker.join("xingshu-prod.db")
    }

    fn staging_dir(temp: &tempfile::TempDir) -> PathBuf {
        let staging = temp.path().join(".staging");
        std::fs::create_dir_all(&staging).expect("staging dir");
        staging
    }

    #[test]
    fn verdict_distinguishes_three_states() {
        let temp = tempfile::tempdir().expect("tempdir");
        let staging = staging_dir(&temp);

        assert_eq!(
            db_path_verdict(&staging.join("dev-001/xingshu-dev.db"), &staging),
            PathVerdict::Inside
        );
        assert_eq!(
            db_path_verdict(&temp.path().join("xingshu-prod.db"), &staging),
            PathVerdict::Outside
        );
        let undetermined = db_path_verdict(&unresolvable_db_path(&temp), &staging);
        assert!(
            matches!(undetermined, PathVerdict::Undetermined { .. }),
            "{undetermined:?}"
        );
    }

    #[test]
    fn verdict_maps_to_error_code() {
        assert_eq!(
            db_path_error_code(&PathVerdict::Inside),
            Some("db_path_in_staging")
        );
        assert_eq!(db_path_error_code(&PathVerdict::Outside), None);
        assert_eq!(
            db_path_error_code(&PathVerdict::Undetermined {
                reason: "unresolvable".to_owned()
            }),
            Some("db_path_unresolvable")
        );
    }

    #[test]
    fn path_prefix_matching_uses_verdict() {
        let base = PathBuf::from("/tmp/xs-case");
        assert_eq!(
            db_path_verdict(
                &base.join(".staging/dev-001/xingshu-dev.db"),
                &base.join(".staging")
            ),
            PathVerdict::Inside
        );
        assert_eq!(
            db_path_verdict(&base.join("xingshu-prod.db"), &base.join(".staging")),
            PathVerdict::Outside
        );
        assert_eq!(
            db_path_verdict(
                &base.join(".staging-backup/xingshu-dev.db"),
                &base.join(".staging")
            ),
            PathVerdict::Outside
        );
        // `path_is_under` stays a coarse boolean view over the same verdict.
        assert!(path_is_under(
            &base.join(".staging/dev-001/xingshu-dev.db"),
            &base.join(".staging")
        ));
        assert!(!path_is_under(
            &base.join("xingshu-prod.db"),
            &base.join(".staging")
        ));
        assert!(!path_is_under(
            &base.join(".staging-backup/xingshu-dev.db"),
            &base.join(".staging")
        ));
    }

    #[test]
    fn dev_never_reaches_verdict() {
        let temp = tempfile::tempdir().expect("tempdir");
        let staging = staging_dir(&temp);

        // dev runs are not policed by the staging guard: the open layer reports real errors.
        assert!(check_db_allowed_under(&staging.join("dev-001/x.db"), false, &staging).is_ok());
        assert!(check_db_allowed_under(&unresolvable_db_path(&temp), false, &staging).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn dangling_alias_is_undetermined_not_inside() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("tempdir");
        let staging = staging_dir(&temp);
        let dangling_alias = temp.path().join("dangling-alias");
        symlink(staging.join("missing"), &dangling_alias).expect("dangling symlink");
        let db = dangling_alias.join("xingshu.db");

        let verdict = db_path_verdict(&db, &staging);
        assert!(
            matches!(verdict, PathVerdict::Undetermined { .. }),
            "{verdict:?}"
        );
        let error = check_db_allowed_under(&db, true, &staging).expect_err("fail closed");
        assert!(error.contains("cannot classify"), "{error}");
        assert!(
            !error.contains("refusing to open staging database"),
            "{error}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn non_directory_component_is_undetermined_and_still_rejected() {
        let temp = tempfile::tempdir().expect("tempdir");
        let staging = staging_dir(&temp);
        let db = unresolvable_db_path(&temp);

        let verdict = db_path_verdict(&db, &staging);
        assert!(
            matches!(verdict, PathVerdict::Undetermined { .. }),
            "{verdict:?}"
        );
        let error = check_db_allowed_under(&db, true, &staging).expect_err("fail closed");
        assert!(error.contains("cannot classify"), "{error}");
        assert!(
            !error.contains("refusing to open staging database"),
            "{error}"
        );
    }
}
