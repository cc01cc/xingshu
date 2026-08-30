use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Utc;

pub struct SizeRollingFile {
    inner: Mutex<RollingState>,
}

struct RollingState {
    path: PathBuf,
    file: Option<File>,
    bytes: u64,
    max_bytes: u64,
    max_files: u32,
    day: String,
}

impl SizeRollingFile {
    pub fn new(path: impl Into<PathBuf>, max_bytes: u64, max_files: u32) -> io::Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = fs::metadata(&path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            inner: Mutex::new(RollingState {
                path,
                file: Some(file),
                bytes,
                max_bytes: max_bytes.max(1),
                max_files: max_files.max(1),
                day: current_day(),
            }),
        })
    }

    fn rotate(state: &mut RollingState, suffix: &str) -> io::Result<()> {
        drop(state.file.take());
        let rotated = PathBuf::from(format!("{}.{}", state.path.display(), suffix));
        if state.path.exists() {
            let _ = fs::remove_file(&rotated);
            fs::rename(&state.path, rotated)?;
        }
        state.file = Some(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&state.path)?,
        );
        state.bytes = 0;
        cleanup_rotated_files(state)
    }
}

impl Write for SizeRollingFile {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| io::Error::other("log writer lock poisoned"))?;
        let day = current_day();
        if state.day != day {
            state.day = day.clone();
            Self::rotate(&mut state, &day)?;
        } else if state.bytes > 0
            && state.bytes.saturating_add(buffer.len() as u64) > state.max_bytes
        {
            Self::rotate(&mut state, "1")?;
        }
        let file = state
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("log file is closed"))?;
        let written = file.write(buffer)?;
        state.bytes = state.bytes.saturating_add(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut state = self
            .inner
            .lock()
            .map_err(|_| io::Error::other("log writer lock poisoned"))?;
        state
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("log file is closed"))?
            .flush()
    }
}

fn cleanup_rotated_files(state: &RollingState) -> io::Result<()> {
    let Some(parent) = state.path.parent() else {
        return Ok(());
    };
    let Some(file_name) = state.path.file_name().and_then(|name| name.to_str()) else {
        return Ok(());
    };
    let mut rotated = fs::read_dir(parent)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(&format!("{file_name}."))
        })
        .collect::<Vec<_>>();
    rotated.sort_by_key(|entry| {
        entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
    });
    let keep = state.max_files as usize;
    if rotated.len() > keep {
        let remove_count = rotated.len() - keep;
        for entry in rotated.into_iter().take(remove_count) {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn current_day() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

pub fn redact(value: &str) -> String {
    let mut result = value.to_owned();
    for marker in ["token=", "access_token=", "password=", "pat="] {
        result = redact_after_marker(&result, marker);
    }
    crate::git::sanitize_remote_url(&result)
}

pub fn redact_path(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "<root>".to_owned())
}

pub fn valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn redact_after_marker(value: &str, marker: &str) -> String {
    let Some(start) = value.to_ascii_lowercase().find(marker) else {
        return value.to_owned();
    };
    let value_start = start + marker.len();
    let value_end = value[value_start..]
        .find(['&', ' ', '\n', '\r'])
        .map(|offset| value_start + offset)
        .unwrap_or(value.len());
    format!("{}***{}", &value[..value_start], &value[value_end..])
}
