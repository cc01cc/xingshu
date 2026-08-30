use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RepoKind {
    ThirdParty,
    ThirdPartyFrozen,
    Fork,
    Own,
}

impl RepoKind {
    pub fn default_modify_lock(&self) -> bool {
        matches!(self, Self::ThirdParty | Self::ThirdPartyFrozen)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ThirdParty => "third-party",
            Self::ThirdPartyFrozen => "third-party-frozen",
            Self::Fork => "fork",
            Self::Own => "own",
        }
    }
}

impl TryFrom<&str> for RepoKind {
    type Error = VcsError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "third-party" => Ok(Self::ThirdParty),
            "third-party-frozen" => Ok(Self::ThirdPartyFrozen),
            "fork" => Ok(Self::Fork),
            "own" => Ok(Self::Own),
            other => Err(VcsError::InvalidRepoKind(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Root {
    pub id: Option<i64>,
    pub name: String,
    pub path: PathBuf,
    pub disk_label: Option<String>,
    pub mount_point: Option<String>,
    pub priority: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoRecord {
    pub id: Option<i64>,
    pub root_id: i64,
    pub org: String,
    pub name: String,
    pub rel_path: PathBuf,
    pub remote_url: Option<String>,
    pub default_branch: Option<String>,
    pub head_commit: Option<String>,
    pub last_fetch_at: Option<DateTime<Utc>>,
    pub last_pull_status: Option<String>,
    pub size_bytes: u64,
    pub is_bare: bool,
    pub clone_status: String,
    pub repo_kind: RepoKind,
    pub modify_lock: bool,
    pub lock_violation: bool,
    pub push_protect_remote: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub id: Option<i64>,
    pub slug: String,
    pub label: String,
    pub color: Option<String>,
    pub kind: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub id: Option<i64>,
    pub repo_id: Option<i64>,
    pub tag_id: Option<i64>,
    pub pull_strategy: String,
    pub fetch_schedule: Option<String>,
    pub depth: String,
    pub auto_tag: bool,
    pub pull_conflict_policy: String,
    pub unattended_conflict_policy: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchLog {
    pub id: Option<i64>,
    pub repo_id: i64,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub strategy: String,
    pub result: String,
    pub objects: Option<u64>,
    pub bytes: Option<u64>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanOptions {
    pub max_depth: Option<usize>,
    pub skip_dirs: Vec<String>,
    pub include_nested: bool,
    pub my_orgs: Vec<String>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_depth: None,
            skip_dirs: [
                "node_modules",
                "target",
                "dist",
                "build",
                ".venv",
                "vendor",
                "out",
                ".git",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            include_nested: false,
            my_orgs: Vec::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum VcsError {
    #[error("invalid repository kind: {0}")]
    InvalidRepoKind(String),
    #[error("repository is locked for Xingshu mutations")]
    ModifyLocked,
    #[error("push to protected remote is denied by Xingshu policy")]
    ProtectedRemote,
    #[error("repository conflict requires a user decision")]
    ConflictNeedsDecision,
    #[error("repository path is outside the configured root")]
    PathOutsideRoot,
    #[error("git command failed: {0}")]
    GitCommand(String),
    #[error("database error: {0}")]
    Database(String),
    #[error("filesystem error: {0}")]
    Filesystem(String),
}
