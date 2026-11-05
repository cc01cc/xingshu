use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

const BUILD_CHANNEL_ENV: &str = "XINGSHU_CLI_BUILD_CHANNEL";
const DATA_DB_ENV: &str = "XINGSHU_DB";
const APP_ENV: &str = "XINGSHU_ENV";

/// Rebuilds the same target directory with both marker values and verifies the
/// compiled CLI's default path. Run explicitly with `--ignored`; it launches Cargo.
#[test]
#[ignore = "nested Cargo build; run explicitly for build-channel regression coverage"]
fn build_channel_flip_recompiles_cli_and_changes_default_database_path() {
    let temp = TempDir::new().expect("tempdir");
    let source_cwd = temp.path().join("source-cwd");
    let release_cwd = temp.path().join("release-cwd");
    let data_root_input = temp.path().join("user-data-input");
    fs::create_dir_all(&source_cwd).expect("source cwd");
    fs::create_dir_all(&release_cwd).expect("release cwd");
    fs::create_dir_all(&data_root_input).expect("data root input");

    let binary = PathBuf::from(env!("CARGO_BIN_EXE_xs"));
    let target_dir = binary
        .parent()
        .and_then(Path::parent)
        .expect("target directory")
        .to_path_buf();
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../Cargo.toml")
        .canonicalize()
        .expect("workspace manifest");
    let original_channel = option_env!("XINGSHU_CLI_BUILD_CHANNEL").map(str::to_owned);

    let invalid_channel_result = run_cargo_build(&manifest_path, &target_dir, Some("production"));
    let release_path_result = build_and_read_db_path(
        &manifest_path,
        &target_dir,
        Some("release"),
        &binary,
        &release_cwd,
        &data_root_input,
    );
    let source_path_result = build_and_read_db_path(
        &manifest_path,
        &target_dir,
        None,
        &binary,
        &source_cwd,
        &data_root_input,
    );
    let restore_result = run_cargo_build(&manifest_path, &target_dir, original_channel.as_deref());

    assert!(
        restore_result.is_ok(),
        "restore original build channel failed: {restore_result:?}"
    );
    assert!(
        invalid_channel_result
            .expect_err("invalid build channel must fail")
            .contains("XINGSHU_CLI_BUILD_CHANNEL must be unset, empty, `source`, or `release`"),
        "invalid build channel must be rejected by build.rs"
    );
    let release_path = release_path_result.expect("release-channel db path");
    let source_path = source_path_result.expect("source-channel db path");

    assert_eq!(release_path.environment, "prod");
    assert_eq!(
        release_path.db_path,
        expected_release_db_path(&data_root_input)
    );
    assert_eq!(source_path.environment, "dev");
    assert_eq!(
        source_path.db_path,
        source_cwd.join(".staging/dev-001/xingshu-dev.db")
    );
    assert!(
        !release_path.db_path.exists(),
        "db path command opened the release DB"
    );
    assert!(
        !source_path.db_path.exists(),
        "db path command opened the source DB"
    );
}

#[derive(Debug)]
struct DbPathOutput {
    environment: String,
    db_path: PathBuf,
}

fn build_and_read_db_path(
    manifest_path: &Path,
    target_dir: &Path,
    build_channel: Option<&str>,
    binary: &Path,
    cwd: &Path,
    data_root: &Path,
) -> Result<DbPathOutput, String> {
    run_cargo_build(manifest_path, target_dir, build_channel)?;

    let mut command = Command::new(binary);
    command
        .current_dir(cwd)
        .args(["db", "path"])
        .env_remove(BUILD_CHANNEL_ENV)
        .env_remove(DATA_DB_ENV)
        .env_remove(APP_ENV)
        .env_remove("XINGSHU_LOG_FILE");
    set_data_root(&mut command, data_root);
    let output = command
        .output()
        .map_err(|error| format!("could not execute xs db path: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "xs db path failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let json: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("xs db path did not return JSON: {error}"))?;
    let environment = json["environment"]
        .as_str()
        .ok_or_else(|| "xs db path omitted environment".to_owned())?
        .to_owned();
    let db_path = json["dbPath"]
        .as_str()
        .ok_or_else(|| "xs db path omitted dbPath".to_owned())?;
    Ok(DbPathOutput {
        environment,
        db_path: PathBuf::from(db_path),
    })
}

fn run_cargo_build(
    manifest_path: &Path,
    target_dir: &Path,
    channel: Option<&str>,
) -> Result<(), String> {
    let mut command = Command::new("cargo");
    command
        .args([
            "build",
            "--offline",
            "--locked",
            "--manifest-path",
            manifest_path.to_str().expect("manifest path"),
            "--target-dir",
            target_dir.to_str().expect("target dir"),
            "-p",
            "xingshu-cli",
            "--bin",
            "xs",
        ])
        .env_remove(BUILD_CHANNEL_ENV);
    if let Some(channel) = channel {
        command.env(BUILD_CHANNEL_ENV, channel);
    }
    let output = command
        .output()
        .map_err(|error| format!("could not start cargo build: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo build failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn expected_release_db_path(data_root: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        data_root.join("Xingshu/xingshu-prod.db")
    }
    #[cfg(target_os = "macos")]
    {
        data_root.join("Library/Application Support/Xingshu/xingshu-prod.db")
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        data_root.join("xingshu/xingshu-prod.db")
    }
}

fn set_data_root(command: &mut Command, data_root: &Path) {
    #[cfg(windows)]
    command.env("LOCALAPPDATA", data_root);
    #[cfg(target_os = "macos")]
    command.env("HOME", data_root);
    #[cfg(all(unix, not(target_os = "macos")))]
    command.env("XDG_DATA_HOME", data_root);
}
