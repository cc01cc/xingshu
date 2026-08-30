use crate::types::{RepoKind, VcsError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullConflictAction {
    Stop,
    Backup,
    Overwrite,
    Abort,
}

impl TryFrom<&str> for PullConflictAction {
    type Error = VcsError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "stop" => Ok(Self::Stop),
            "backup" => Ok(Self::Backup),
            "overwrite" => Ok(Self::Overwrite),
            "abort" => Ok(Self::Abort),
            other => Err(VcsError::GitCommand(format!(
                "invalid pull conflict policy: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedPolicy {
    pub pull_strategy: String,
    pub conflict_action: PullConflictAction,
    pub unattended_action: PullConflictAction,
}

impl Default for ResolvedPolicy {
    fn default() -> Self {
        Self {
            pull_strategy: "fetch-only".to_owned(),
            conflict_action: PullConflictAction::Stop,
            unattended_action: PullConflictAction::Abort,
        }
    }
}

pub fn default_for_kind(kind: &RepoKind) -> ResolvedPolicy {
    let mut policy = ResolvedPolicy::default();
    if matches!(kind, RepoKind::ThirdPartyFrozen) {
        policy.pull_strategy = "no-update".to_owned();
    }
    policy
}

pub fn is_update_enabled(strategy: &str) -> bool {
    matches!(strategy, "fetch-only" | "mirror")
}
