use std::path::Path;

use crate::git::{self, GitCommandOutput, GitMetadata};
use crate::types::VcsError;

pub trait Vcs {
    fn inspect(&self, path: &Path) -> Result<GitMetadata, VcsError>;
    fn run(&self, path: &Path, args: &[&str]) -> Result<GitCommandOutput, VcsError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct GitBackend;

impl Vcs for GitBackend {
    fn inspect(&self, path: &Path) -> Result<GitMetadata, VcsError> {
        git::inspect(path)
    }

    fn run(&self, path: &Path, args: &[&str]) -> Result<GitCommandOutput, VcsError> {
        git::run_git(path, args)
    }
}
