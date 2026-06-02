//! Agent domain tools (3): spawn, status, message

use super::handler::{ToolHandler, ExecuteFuture};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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

// ============================================================================
// Stub handlers for agent tools
// ============================================================================

pub struct AgentSpawnStub;

impl ToolHandler for AgentSpawnStub {
    fn name(&self) -> &'static str {
        "spawn"
    }

    fn domain(&self) -> &'static str {
        "agent"
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        // TODO: Validate required fields: host, archetype, prompt, model
        Ok(())
    }

    fn execute(&self, _params: Value) -> ExecuteFuture {
        Box::pin(async move {
            // TODO: Route to ruflo-host, create agent instance
            let agent_id = Uuid::new_v4().to_string();
            Ok(json!({
                "agent_id": agent_id,
                "state": "spawned",
            }))
        })
    }
}

pub struct AgentStatusStub;

impl ToolHandler for AgentStatusStub {
    fn name(&self) -> &'static str {
        "status"
    }

    fn domain(&self) -> &'static str {
        "agent"
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }

    fn execute(&self, _params: Value) -> ExecuteFuture {
        Box::pin(async move {
            // TODO: Query agent registry from ruflo-host
            Ok(json!({
                "agents": [],
            }))
        })
    }
}

pub struct AgentMessageStub;

impl ToolHandler for AgentMessageStub {
    fn name(&self) -> &'static str {
        "message"
    }

    fn domain(&self) -> &'static str {
        "agent"
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        // TODO: Validate required fields: agent_id, message
        Ok(())
    }

    fn execute(&self, _params: Value) -> ExecuteFuture {
        Box::pin(async move {
            // TODO: Route to agent's message queue
            Ok(json!({
                "status": "sent",
            }))
        })
    }
}
