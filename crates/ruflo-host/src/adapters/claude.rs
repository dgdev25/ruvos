//! Claude Code CLI host adapter.
//!
//! Provides normalized event forwarding to the Claude Code CLI.
//! Phase 6v1: Simple event-forwarding implementation.
//! Real task management and streaming will be added in Phase 6 refinement.

use crate::host::{AgentEvent, AgentRequest, CliError, CliHost, ModelSpec, ToolCall, ToolResponse};
use async_trait::async_trait;
use serde_json::json;
use std::sync::{Arc, Mutex};

/// Adapter for Claude Code CLI.
/// Stores recent events in memory for round-trip validation.
pub struct ClaudeHost {
    /// Buffered events for this session
    events: Arc<Mutex<Vec<AgentEvent>>>,
    /// Buffered tool responses for round-trip
    responses: Arc<Mutex<Vec<ToolResponse>>>,
}

impl ClaudeHost {
    /// Create a new Claude Code host adapter.
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl Default for ClaudeHost {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CliHost for ClaudeHost {
    fn name(&self) -> &str {
        "claude"
    }

    async fn available_models(&self) -> anyhow::Result<Vec<ModelSpec>> {
        Ok(vec![
            ModelSpec::new("claude-opus", 3, 200000),
            ModelSpec::new("claude-sonnet", 2, 200000),
            ModelSpec::new("claude-haiku", 1, 100000),
        ])
    }

    async fn run(&self, request: AgentRequest) -> anyhow::Result<String> {
        // Forward request metadata as an event
        let event = AgentEvent::Started {
            agent_id: format!("claude-agent-{}", uuid::Uuid::new_v4()),
        };
        self.events.lock().unwrap().push(event);

        // Log the request archetype and model
        let output_event = AgentEvent::Output {
            text: format!(
                "Running {} agent with model {} (budget: {})",
                request.archetype, request.model, request.budget
            ),
        };
        self.events.lock().unwrap().push(output_event);

        // Return success marker
        let result = "Agent completed successfully".to_string();
        let completed = AgentEvent::Completed {
            result: result.clone(),
        };
        self.events.lock().unwrap().push(completed);

        Ok(result)
    }

    async fn stream(&self, request: AgentRequest) -> anyhow::Result<Vec<AgentEvent>> {
        // Generate a sequence of events representing agent execution
        let mut events = Vec::new();

        let agent_id = format!("claude-agent-{}", uuid::Uuid::new_v4());
        events.push(AgentEvent::Started {
            agent_id: agent_id.clone(),
        });

        events.push(AgentEvent::Output {
            text: format!(
                "Streaming {} agent with model {}",
                request.archetype, request.model
            ),
        });

        events.push(AgentEvent::Output {
            text: format!("Prompt: {}", request.prompt),
        });

        for trait_name in &request.traits {
            events.push(AgentEvent::Output {
                text: format!("Applying trait: {}", trait_name),
            });
        }

        events.push(AgentEvent::Completed {
            result: "Agent stream completed".to_string(),
        });

        // Store events in buffer
        for event in &events {
            self.events.lock().unwrap().push(event.clone());
        }

        Ok(events)
    }

    async fn send_tool_call(&self, tool_call: ToolCall) -> anyhow::Result<()> {
        // Log the tool call
        let event = AgentEvent::Output {
            text: format!(
                "Claude: Calling {} with params: {}",
                tool_call.method,
                serde_json::to_string(&tool_call.params)?
            ),
        };
        self.events.lock().unwrap().push(event);
        Ok(())
    }

    async fn receive_response(&self) -> anyhow::Result<ToolResponse> {
        // Try to pop a buffered response; if none exist, return a default
        let response = self
            .responses
            .lock()
            .unwrap()
            .pop()
            .unwrap_or_else(|| ToolResponse {
                id: uuid::Uuid::new_v4().to_string(),
                result: Some(json!({ "status": "ok" })),
                error: None,
            });

        let event = AgentEvent::Output {
            text: format!("Claude: Received response for tool call: {}", response.id),
        };
        self.events.lock().unwrap().push(event);

        Ok(response)
    }

    async fn report_error(&self, error: CliError) -> anyhow::Result<()> {
        let event = AgentEvent::Error {
            message: format!("Claude error (code {}): {}", error.code, error.message),
        };
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}
