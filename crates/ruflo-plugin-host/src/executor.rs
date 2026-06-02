//! Async shell command executor for plugin invocation.

use crate::types::{ExecutionRequest, ExecutionResult};
use crate::Result;
use std::process::Stdio;
use tokio::process::Command;

/// Executes shell commands asynchronously on behalf of plugins.
#[derive(Debug)]
pub struct PluginExecutor;

impl PluginExecutor {
    /// Creates a new PluginExecutor instance.
    pub fn new() -> Self {
        PluginExecutor
    }

    /// Executes a shell command and returns the result with status, stdout, and stderr.
    pub async fn execute(&self, request: &ExecutionRequest) -> Result<ExecutionResult> {
        let mut cmd = Command::new(&request.command);
        cmd.args(&request.args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd.output().await.map_err(|e| {
            crate::error::PluginError::ExecutionFailed(format!(
                "failed to execute '{}': {}",
                request.command, e
            ))
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let status = output.status.code().unwrap_or(1);

        Ok(ExecutionResult {
            status,
            stdout,
            stderr,
        })
    }
}

impl Default for PluginExecutor {
    fn default() -> Self {
        Self::new()
    }
}
