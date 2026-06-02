//! Codex CLI host adapter.

use crate::host::{CliHost, ModelSpec, AgentRequest, AgentEvent};
use async_trait::async_trait;

/// Adapter for Codex CLI.
pub struct CodexHost {
    // TODO: Binary invocation or IPC to Codex
}

#[async_trait]
impl CliHost for CodexHost {
    fn name(&self) -> &str {
        "codex"
    }

    async fn available_models(&self) -> anyhow::Result<Vec<ModelSpec>> {
        // TODO: Query Codex for available models
        Ok(vec![ModelSpec::new("codex-mini", 1, 50000)])
    }

    async fn run(&self, _request: AgentRequest) -> anyhow::Result<String> {
        // TODO: Invoke Codex CLI, capture output
        Ok(String::new())
    }

    async fn stream(&self, _request: AgentRequest) -> anyhow::Result<Vec<AgentEvent>> {
        // TODO: Stream events from Codex execution
        Ok(vec![])
    }
}
