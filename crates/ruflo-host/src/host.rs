//! Core CliHost trait definition and types.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Model specification (name, tier, context window).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    pub name: String,
    pub tier: u32,
    pub context_window: u32,
}

/// Agent request parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    pub archetype: String,
    pub prompt: String,
    pub traits: Vec<String>,
    pub model: String,
    pub budget: u32,
}

/// Agent event stream (normalized across hosts).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AgentEvent {
    Started { agent_id: String },
    Output { text: String },
    Error { message: String },
    Completed { result: String },
}

/// Tool call request (MCP round-trip).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub method: String,
    pub params: serde_json::Value,
}

/// Tool call response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResponse {
    pub id: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// CLI error event (structured error reporting).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliError {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}

/// CliHost trait: abstraction over Claude Code, Codex, Gemini CLIs.
#[async_trait]
pub trait CliHost: Send + Sync {
    /// Get the host's display name.
    fn name(&self) -> &str;

    /// List available models.
    async fn available_models(&self) -> anyhow::Result<Vec<ModelSpec>>;

    /// Run an agent request on this host.
    async fn run(&self, request: AgentRequest) -> anyhow::Result<String>;

    /// Stream agent events (used for multi-turn interactions).
    async fn stream(&self, request: AgentRequest) -> anyhow::Result<Vec<AgentEvent>>;

    /// Send a tool call to the host.
    async fn send_tool_call(&self, tool_call: ToolCall) -> anyhow::Result<()>;

    /// Receive a tool response from the host.
    async fn receive_response(&self) -> anyhow::Result<ToolResponse>;

    /// Report an error to the host.
    async fn report_error(&self, error: CliError) -> anyhow::Result<()>;
}

/// Default ModelSpec factory.
impl ModelSpec {
    pub fn new(name: &str, tier: u32, context_window: u32) -> Self {
        Self {
            name: name.to_string(),
            tier,
            context_window,
        }
    }
}
