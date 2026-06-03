//! Agent domain tools (3): spawn, status, message
//!
//! Manages agent lifecycle: creation, status queries, and inter-agent messaging.
//! Phase 5v1 uses in-memory registry; Phase 6 integrates with CliHost for real execution.

use super::handler::{ExecuteFuture, ToolHandler};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use uuid::Uuid;

// Valid agent archetypes from scope ledger
const VALID_ARCHETYPES: &[&str] = &[
    "coder",
    "reviewer",
    "tester",
    "researcher",
    "architect",
    "planner",
    "security",
    "perf",
    "devops",
    "data",
    "docs",
    "coordinator",
];

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub id: String,
    pub archetype: String,
    pub traits: Vec<String>,
    pub status: String,
    pub created_at: String,
    pub last_message_at: Option<String>,
    pub message_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub content: String,
    pub timestamp: String,
}

/// Global in-memory agent registry for Phase 5v1.
/// Maps agent_id -> AgentState with message queues.
static AGENT_REGISTRY: Mutex<Option<AgentRegistry>> = Mutex::new(None);

pub struct AgentRegistry {
    agents: HashMap<String, AgentState>,
    message_queues: HashMap<String, VecDeque<Message>>,
}

impl AgentRegistry {
    fn new() -> Self {
        AgentRegistry {
            agents: HashMap::new(),
            message_queues: HashMap::new(),
        }
    }

    fn get_or_init() -> &'static Mutex<Option<AgentRegistry>> {
        // Ensure registry is initialized once
        if AGENT_REGISTRY.lock().unwrap().is_none() {
            *AGENT_REGISTRY.lock().unwrap() = Some(AgentRegistry::new());
        }
        &AGENT_REGISTRY
    }

    fn spawn_agent(
        &mut self,
        archetype: String,
        traits: Vec<String>,
        _prompt: String,
        _model: String,
        _budget: u32,
    ) -> Result<AgentState> {
        let agent_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let agent = AgentState {
            id: agent_id.clone(),
            archetype,
            traits,
            status: "active".to_string(),
            created_at: now,
            last_message_at: None,
            message_count: 0,
        };

        self.agents.insert(agent_id.clone(), agent.clone());
        self.message_queues
            .insert(agent_id.clone(), VecDeque::new());

        Ok(agent)
    }

    fn get_agent(&self, agent_id: &str) -> Option<AgentState> {
        self.agents.get(agent_id).cloned()
    }

    fn list_agents(&self) -> Vec<AgentState> {
        self.agents.values().cloned().collect()
    }

    fn enqueue_message(&mut self, agent_id: &str, content: String) -> Result<String> {
        let message_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let message = Message {
            id: message_id.clone(),
            content,
            timestamp: now.clone(),
        };

        if let Some(queue) = self.message_queues.get_mut(agent_id) {
            queue.push_back(message);
        } else {
            return Err(crate::RuvosError::ValidationError(format!(
                "Agent not found: {}",
                agent_id
            )));
        }

        // Update agent last_message_at and increment counter
        if let Some(agent) = self.agents.get_mut(agent_id) {
            agent.last_message_at = Some(now);
            agent.message_count += 1;
        }

        Ok(message_id)
    }
}

// ============================================================================
// Agent tool handlers
// ============================================================================

pub struct AgentSpawnHandler;

impl ToolHandler for AgentSpawnHandler {
    fn name(&self) -> &'static str {
        "spawn"
    }

    fn domain(&self) -> &'static str {
        "agent"
    }

    fn validate(&self, params: &Value) -> Result<()> {
        // Required fields
        params
            .get("archetype")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::RuvosError::ValidationError("Missing: archetype".into()))?;

        params
            .get("prompt")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::RuvosError::ValidationError("Missing: prompt".into()))?;

        params
            .get("model")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::RuvosError::ValidationError("Missing: model".into()))?;

        // Validate archetype
        let archetype = params.get("archetype").unwrap().as_str().unwrap();
        if !VALID_ARCHETYPES.contains(&archetype) {
            return Err(crate::RuvosError::ValidationError(format!(
                "Invalid archetype: {}. Must be one of: {}",
                archetype,
                VALID_ARCHETYPES.join(", ")
            )));
        }

        // Optional budget validation
        if let Some(budget) = params.get("budget").and_then(|v| v.as_u64()) {
            if budget == 0 {
                return Err(crate::RuvosError::ValidationError(
                    "Budget must be > 0".into(),
                ));
            }
        }

        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let archetype = params
                .get("archetype")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            let traits: Vec<String> = params
                .get("traits")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();
            let prompt = params.get("prompt").unwrap().as_str().unwrap().to_string();
            let model = params.get("model").unwrap().as_str().unwrap().to_string();
            let budget = params
                .get("budget")
                .and_then(|v| v.as_u64())
                .unwrap_or(1000) as u32;

            let registry = AgentRegistry::get_or_init();
            let mut reg = registry.lock().unwrap();
            if let Some(ref mut r) = *reg {
                let agent = r.spawn_agent(archetype, traits, prompt, model, budget)?;
                Ok(json!({
                    "agent_id": agent.id,
                    "archetype": agent.archetype,
                    "status": agent.status,
                    "created_at": agent.created_at,
                }))
            } else {
                Err(crate::RuvosError::InternalError(
                    "Registry not initialized".into(),
                ))
            }
        })
    }
}

pub struct AgentStatusHandler;

impl ToolHandler for AgentStatusHandler {
    fn name(&self) -> &'static str {
        "status"
    }

    fn domain(&self) -> &'static str {
        "agent"
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        // agent_id is optional
        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let agent_id = params.get("agent_id").and_then(|v| v.as_str());

            let registry = AgentRegistry::get_or_init();
            let reg = registry.lock().unwrap();

            if let Some(ref r) = *reg {
                if let Some(id) = agent_id {
                    // Return single agent
                    if let Some(agent) = r.get_agent(id) {
                        Ok(json!({
                            "agent_id": agent.id,
                            "archetype": agent.archetype,
                            "traits": agent.traits,
                            "status": agent.status,
                            "created_at": agent.created_at,
                            "last_message_at": agent.last_message_at,
                            "message_count": agent.message_count,
                        }))
                    } else {
                        Err(crate::RuvosError::ValidationError(format!(
                            "Agent not found: {}",
                            id
                        )))
                    }
                } else {
                    // Return all agents
                    let agents = r.list_agents();
                    let agent_list: Vec<Value> = agents
                        .iter()
                        .map(|agent| {
                            json!({
                                "agent_id": agent.id,
                                "archetype": agent.archetype,
                                "traits": agent.traits,
                                "status": agent.status,
                                "created_at": agent.created_at,
                                "last_message_at": agent.last_message_at,
                                "message_count": agent.message_count,
                            })
                        })
                        .collect();

                    Ok(json!({
                        "agents": agent_list,
                        "total": agent_list.len(),
                    }))
                }
            } else {
                Err(crate::RuvosError::InternalError(
                    "Registry not initialized".into(),
                ))
            }
        })
    }
}

pub struct AgentMessageHandler;

impl ToolHandler for AgentMessageHandler {
    fn name(&self) -> &'static str {
        "message"
    }

    fn domain(&self) -> &'static str {
        "agent"
    }

    fn validate(&self, params: &Value) -> Result<()> {
        params
            .get("agent_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::RuvosError::ValidationError("Missing: agent_id".into()))?;

        params
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| crate::RuvosError::ValidationError("Missing: message".into()))?;

        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let agent_id = params
                .get("agent_id")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
            let message = params.get("message").unwrap().as_str().unwrap().to_string();

            let registry = AgentRegistry::get_or_init();
            let mut reg = registry.lock().unwrap();

            if let Some(ref mut r) = *reg {
                let message_id = r.enqueue_message(&agent_id, message)?;
                Ok(json!({
                    "message_id": message_id,
                    "agent_id": agent_id,
                    "status": "enqueued",
                }))
            } else {
                Err(crate::RuvosError::InternalError(
                    "Registry not initialized".into(),
                ))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spawn_validation_missing_archetype() {
        let handler = AgentSpawnHandler;
        let params = json!({
            "prompt": "test",
            "model": "claude-3-sonnet"
        });
        assert!(handler.validate(&params).is_err());
    }

    #[test]
    fn test_spawn_validation_missing_prompt() {
        let handler = AgentSpawnHandler;
        let params = json!({
            "archetype": "coder",
            "model": "claude-3-sonnet"
        });
        assert!(handler.validate(&params).is_err());
    }

    #[test]
    fn test_spawn_validation_missing_model() {
        let handler = AgentSpawnHandler;
        let params = json!({
            "archetype": "coder",
            "prompt": "test"
        });
        assert!(handler.validate(&params).is_err());
    }

    #[test]
    fn test_spawn_validation_invalid_archetype() {
        let handler = AgentSpawnHandler;
        let params = json!({
            "archetype": "invalid_archetype",
            "prompt": "test",
            "model": "claude-3-sonnet"
        });
        assert!(handler.validate(&params).is_err());
    }

    #[test]
    fn test_spawn_validation_valid() {
        let handler = AgentSpawnHandler;
        let params = json!({
            "archetype": "coder",
            "prompt": "test",
            "model": "claude-3-sonnet"
        });
        assert!(handler.validate(&params).is_ok());
    }

    #[test]
    fn test_spawn_validation_all_archetypes() {
        let handler = AgentSpawnHandler;
        for archetype in VALID_ARCHETYPES {
            let params = json!({
                "archetype": archetype,
                "prompt": "test",
                "model": "claude-3-sonnet"
            });
            assert!(
                handler.validate(&params).is_ok(),
                "Archetype {} should be valid",
                archetype
            );
        }
    }

    #[test]
    fn test_spawn_validation_with_traits() {
        let handler = AgentSpawnHandler;
        let params = json!({
            "archetype": "coder",
            "prompt": "test",
            "model": "claude-3-sonnet",
            "traits": ["tdd", "backend"]
        });
        assert!(handler.validate(&params).is_ok());
    }

    #[test]
    fn test_spawn_validation_with_budget() {
        let handler = AgentSpawnHandler;
        let params = json!({
            "archetype": "coder",
            "prompt": "test",
            "model": "claude-3-sonnet",
            "budget": 5000
        });
        assert!(handler.validate(&params).is_ok());
    }

    #[test]
    fn test_spawn_validation_zero_budget() {
        let handler = AgentSpawnHandler;
        let params = json!({
            "archetype": "coder",
            "prompt": "test",
            "model": "claude-3-sonnet",
            "budget": 0
        });
        assert!(handler.validate(&params).is_err());
    }

    #[tokio::test]
    async fn test_spawn_handler_execute() {
        let handler = AgentSpawnHandler;
        let params = json!({
            "archetype": "coder",
            "prompt": "implement a function",
            "model": "claude-3-haiku",
            "traits": ["backend"]
        });

        let result = handler.execute(params).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.get("agent_id").is_some());
        assert_eq!(
            response.get("archetype").and_then(|v| v.as_str()),
            Some("coder")
        );
        assert_eq!(
            response.get("status").and_then(|v| v.as_str()),
            Some("active")
        );
        assert!(response.get("created_at").is_some());
    }

    #[test]
    fn test_message_validation_missing_agent_id() {
        let handler = AgentMessageHandler;
        let params = json!({
            "message": "hello"
        });
        assert!(handler.validate(&params).is_err());
    }

    #[test]
    fn test_message_validation_missing_message() {
        let handler = AgentMessageHandler;
        let params = json!({
            "agent_id": "test-id"
        });
        assert!(handler.validate(&params).is_err());
    }

    #[test]
    fn test_message_validation_valid() {
        let handler = AgentMessageHandler;
        let params = json!({
            "agent_id": "test-id",
            "message": "hello"
        });
        assert!(handler.validate(&params).is_ok());
    }

    #[test]
    fn test_status_validation() {
        let handler = AgentStatusHandler;
        // No required fields
        assert!(handler.validate(&json!({})).is_ok());
        assert!(handler.validate(&json!({"agent_id": "test"})).is_ok());
    }
}
