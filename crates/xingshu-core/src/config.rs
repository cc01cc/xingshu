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

pub fn check_db_allowed(path: &Path) -> Result<(), String> {
    check_db_allowed_with(path, is_prod())
}

pub fn check_db_allowed_with(path: &Path, prod: bool) -> Result<(), String> {
    if prod && path_is_under(path, &staging_root()) {
        return Err(format!(
            "refusing to open staging database {} with {}=prod; unset {} or use a dev run",
            path.display(),
            ENV_NAME,
            DB_ENV_VAR
        ));
    }
    Ok(())
}

pub fn staging_root() -> PathBuf {
    to_absolute(Path::new(STAGING_DIR))
}

pub fn path_is_under(path: &Path, dir: &Path) -> bool {
    to_absolute(path).starts_with(to_absolute(dir))
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
    fn prod_refuses_staging_db() {
        let staging_db = staging_root().join("dev-002/xingshu-dev.db");
        assert!(check_db_allowed_with(&staging_db, true).is_err());
        assert!(check_db_allowed_with(Path::new("xingshu-prod.db"), true).is_ok());
        assert!(check_db_allowed_with(&staging_db, false).is_ok());
    }

    #[test]
    fn path_prefix_matching() {
        let base = PathBuf::from("/tmp/xs-case");
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
}
