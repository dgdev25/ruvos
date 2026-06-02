//! Codex CLI host adapter.

use crate::host::{AgentEvent, AgentRequest, CliError, CliHost, ModelSpec, ToolCall, ToolResponse};
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

    async fn send_tool_call(&self, _tool_call: ToolCall) -> anyhow::Result<()> {
        // TODO: Forward tool call to Codex
        Ok(())
    }

    async fn receive_response(&self) -> anyhow::Result<ToolResponse> {
        // TODO: Receive tool response from Codex
        Ok(ToolResponse {
            id: String::new(),
            result: None,
            error: Some("not_implemented".to_string()),
        })
    }

    async fn report_error(&self, _error: CliError) -> anyhow::Result<()> {
        // TODO: Report error to Codex
        Ok(())
    }
}
