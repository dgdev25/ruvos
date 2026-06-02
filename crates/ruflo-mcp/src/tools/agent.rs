//! Agent domain tools (3): spawn, status, message

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    pub host: String,
    pub archetype: String,
    pub prompt: String,
    pub traits: Vec<String>,
    pub model: String,
    pub budget: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    pub id: String,
    pub archetype: String,
    pub state: String,
}

/// Spawn a host agent.
pub async fn spawn(_request: AgentRequest) -> anyhow::Result<String> {
    let agent_id = Uuid::new_v4().to_string();
    // TODO: Route to ruflo-host, create agent instance
    Ok(agent_id)
}

/// List running agents + states.
pub async fn status() -> anyhow::Result<Vec<AgentStatus>> {
    // TODO: Query agent registry from ruflo-host
    Ok(vec![])
}

/// Send message to a named agent.
pub async fn message(_agent_id: &str, _message: &str) -> anyhow::Result<String> {
    // TODO: Route to agent's message queue
    Ok(String::new())
}
