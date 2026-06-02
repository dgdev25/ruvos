//! Claude Code CLI host adapter.

use crate::host::{AgentEvent, AgentRequest, CliError, CliHost, ModelSpec, ToolCall, ToolResponse};
use async_trait::async_trait;

/// Adapter for Claude Code CLI.
pub struct ClaudeHost {
    // TODO: Socket/IPC to Claude Code daemon
}

#[async_trait]
impl CliHost for ClaudeHost {
    fn name(&self) -> &str {
        "claude"
    }

    async fn available_models(&self) -> anyhow::Result<Vec<ModelSpec>> {
        // TODO: Query Claude Code for available models
        Ok(vec![
            ModelSpec::new("claude-opus", 3, 200000),
            ModelSpec::new("claude-sonnet", 2, 200000),
            ModelSpec::new("claude-haiku", 1, 100000),
        ])
    }

    async fn run(&self, _request: AgentRequest) -> anyhow::Result<String> {
        // TODO: Forward to Claude Code daemon, wait for result
        Ok(String::new())
    }

    async fn stream(&self, _request: AgentRequest) -> anyhow::Result<Vec<AgentEvent>> {
        // TODO: Stream events from Claude Code daemon
        Ok(vec![])
    }

    async fn send_tool_call(&self, _tool_call: ToolCall) -> anyhow::Result<()> {
        // TODO: Forward tool call to Claude Code daemon
        Ok(())
    }

    async fn receive_response(&self) -> anyhow::Result<ToolResponse> {
        // TODO: Receive tool response from Claude Code daemon
        Ok(ToolResponse {
            id: String::new(),
            result: None,
            error: Some("not_implemented".to_string()),
        })
    }

    async fn report_error(&self, _error: CliError) -> anyhow::Result<()> {
        // TODO: Report error to Claude Code daemon
        Ok(())
    }
}
