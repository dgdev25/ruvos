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
