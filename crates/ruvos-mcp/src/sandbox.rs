//! Git worktree sandbox helpers for isolated agent execution.

use std::path::PathBuf;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::{Result, RuvosError};

fn validate_branch_name(branch: &str) -> Result<()> {
    if branch.is_empty() {
        return Err(RuvosError::InternalError(
            "branch name must not be empty".to_string(),
        ));
    }
    if !branch
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '/' | '.'))
    {
        return Err(RuvosError::InternalError(format!(
            "branch name contains invalid characters: {branch}"
        )));
    }
    if branch.starts_with('-') || branch.starts_with('.') {
        return Err(RuvosError::InternalError(format!(
            "branch name must not start with '-' or '.': {branch}"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorktreeSandbox {
    pub repo_root: PathBuf,
    pub worktree_path: PathBuf,
    pub branch: String,
}

impl WorktreeSandbox {
    pub fn new(
        repo_root: impl Into<PathBuf>,
        worktree_path: impl Into<PathBuf>,
        branch: impl Into<String>,
    ) -> Self {
        Self {
            repo_root: repo_root.into(),
            worktree_path: worktree_path.into(),
            branch: branch.into(),
        }
    }

    pub fn create(&self) -> Result<()> {
        validate_branch_name(&self.branch)?;

        if let Some(parent) = self.worktree_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                RuvosError::InternalError(format!("worktree parent create failed: {error}"))
            })?;
        }

        let status = Command::new("git")
            .args([
                "-C",
                self.repo_root
                    .to_str()
                    .ok_or_else(|| RuvosError::InternalError("repo root not utf-8".to_string()))?,
                "worktree",
                "add",
                "-b",
                &self.branch,
                self.worktree_path.to_str().ok_or_else(|| {
                    RuvosError::InternalError("worktree path not utf-8".to_string())
                })?,
                "HEAD",
            ])
            .status()
            .map_err(|error| RuvosError::InternalError(format!("worktree add failed: {error}")))?;

        if status.success() {
            Ok(())
        } else {
            Err(RuvosError::InternalError(
                "git worktree add returned non-zero".to_string(),
            ))
        }
    }

    pub fn cleanup(&self) -> Result<()> {
        let _ = Command::new("git")
            .args([
                "-C",
                self.repo_root
                    .to_str()
                    .ok_or_else(|| RuvosError::InternalError("repo root not utf-8".to_string()))?,
                "worktree",
                "remove",
                "--force",
                self.worktree_path.to_str().ok_or_else(|| {
                    RuvosError::InternalError("worktree path not utf-8".to_string())
                })?,
            ])
            .status();
        let _ = std::fs::remove_dir_all(&self.worktree_path);
        Ok(())
    }
}

#[cfg(test)]
fn git(args: &[&str], cwd: &std::path::Path) -> Result<()> {
    let status = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .map_err(|error| RuvosError::InternalError(format!("git command failed: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(RuvosError::InternalError(format!(
            "git command failed: {:?}",
            args
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn init_repo() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(&["init"], dir.path()).unwrap();
        git(&["config", "user.email", "codex@example.com"], dir.path()).unwrap();
        git(&["config", "user.name", "Codex"], dir.path()).unwrap();
        std::fs::write(dir.path().join("README.md"), "sandbox").unwrap();
        git(&["add", "README.md"], dir.path()).unwrap();
        git(&["commit", "-m", "init"], dir.path()).unwrap();
        dir
    }

    #[test]
    fn worktree_sandbox_creates_and_cleans_up() {
        let repo = init_repo();
        let worktree_path = repo.path().join("wt");
        let sandbox = WorktreeSandbox::new(repo.path(), &worktree_path, "codex/sandbox");
        sandbox.create().unwrap();
        assert!(worktree_path.exists());
        sandbox.cleanup().unwrap();
    }
}
