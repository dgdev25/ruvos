use super::artifact::{extract_structured_output, VALID_ARCHETYPES};
use super::task::run_task;
use super::transport::{register_agent_transport, transport_send, TRANSPORT_REGISTRY};
use crate::runtime::{publish_event, RuntimeEvent};
use crate::skills::{select_skill_bundle, SkillBundle};
use crate::tools::agent_store;
use crate::tools::handler::{ExecuteFuture, ToolHandler};
use crate::{Result, RuvosError};
use serde_json::{json, Value};
use uuid::Uuid;

// ============================================================================
// agent.spawn
// ============================================================================

pub struct AgentSpawnHandler;

impl ToolHandler for AgentSpawnHandler {
    fn name(&self) -> &'static str {
        "spawn"
    }
    fn domain(&self) -> &'static str {
        "agent"
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "archetype": {
                    "type": "string",
                    "enum": ["coder", "reviewer", "tester"],
                    "description": "Agent archetype controlling the task focus and artifact style"
                },
                "prompt": {
                    "type": "string",
                    "description": "Task description or goal for the agent"
                },
                "model": {
                    "type": "string",
                    "description": "Model ID to use for this agent (e.g. claude-sonnet-4-6)"
                },
                "traits": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional skill tags to influence behavior"
                },
                "output_schema": {
                    "type": "object",
                    "description": "Optional JSON Schema. When provided, agent output includes a structured JSON block returned as structured_output.",
                    "additionalProperties": true
                }
            },
            "required": ["archetype", "prompt", "model"]
        })
    }

    fn validate(&self, params: &Value) -> Result<()> {
        for field in ["archetype", "prompt", "model"] {
            if params.get(field).and_then(|v| v.as_str()).is_none() {
                return Err(RuvosError::InvalidParams(format!(
                    "missing '{}' field (string)",
                    field
                )));
            }
        }
        let archetype = params["archetype"].as_str().unwrap_or_default();
        if !VALID_ARCHETYPES.contains(&archetype) {
            return Err(RuvosError::InvalidParams(format!(
                "invalid archetype '{}'; must be one of {:?}",
                archetype, VALID_ARCHETYPES
            )));
        }
        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let archetype = params["archetype"].as_str().unwrap_or_default().to_string();
            let base_prompt = params["prompt"].as_str().unwrap_or_default().to_string();
            let model = params["model"].as_str().unwrap_or_default().to_string();

            // ADR-034 phase 2: enrich the prompt with AISP notation before dispatch.
            let (effective_base, aisp_json) = {
                let enriched = crate::tools::aisp_layer::enrich(
                    &base_prompt,
                    &crate::tools::aisp_layer::AispConfig::load(),
                );
                if enriched.assessment.as_ref().is_some_and(|a| a.blocked) {
                    let a = enriched.assessment.expect("blocked implies Some");
                    return Ok(json!({
                        "status": "blocked",
                        "reason": "aisp_quality_gate",
                        "aisp": a.to_json(),
                    }));
                }
                let j = enriched.assessment.map(|a| a.to_json());
                (enriched.effective_prompt, j)
            };
            let traits: Vec<String> = params
                .get("traits")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let provided_skill_bundle: Option<SkillBundle> = params
                .get("skill_bundle")
                .and_then(|value| {
                    if value.is_null() {
                        None
                    } else {
                        Some(value.clone())
                    }
                })
                .map(|value| {
                    serde_json::from_value(value).map_err(|error| {
                        RuvosError::InvalidParams(format!("invalid skill_bundle: {error}"))
                    })
                })
                .transpose()?;
            let skill_bundle = match provided_skill_bundle {
                Some(bundle) => Some(bundle),
                None => match select_skill_bundle(&archetype, &base_prompt, 3) {
                    Ok(bundle) => bundle,
                    Err(error) => {
                        tracing::debug!("skill selection unavailable for {}: {}", archetype, error);
                        None
                    }
                },
            };
            let prompt = if let Some(bundle) = &skill_bundle {
                format!(
                    "{effective_base}\n\n{}\n",
                    bundle.render_prompt_section().trim_end()
                )
            } else {
                effective_base.clone()
            };
            let selected_skills = skill_bundle.clone();

            let agent_id = Uuid::new_v4().to_string();
            publish_event(RuntimeEvent {
                kind: "agent.spawn.started".to_string(),
                payload: json!({
                    "agent_id": agent_id.clone(),
                    "archetype": archetype.clone(),
                    "model": model.clone(),
                    "prompt": base_prompt.clone(),
                    "selected_skills": &selected_skills,
                }),
                agent_id: Some(agent_id.clone()),
                task_id: None,
            });

            let runner = params.get("runner").and_then(|value| value.as_str());
            let output_schema = params
                .get("output_schema")
                .filter(|v| !v.is_null())
                .cloned();

            let outcome = match run_task(
                &agent_id,
                &archetype,
                &prompt,
                runner,
                output_schema.clone(),
            )
            .await
            {
                Ok(outcome) => outcome,
                Err(error) => {
                    publish_event(RuntimeEvent {
                        kind: "agent.spawn.failed".to_string(),
                        payload: json!({
                            "agent_id": agent_id.clone(),
                            "archetype": archetype.clone(),
                            "model": model.clone(),
                            "prompt": base_prompt.clone(),
                            "selected_skills": &selected_skills,
                            "error": format!("{error:?}"),
                        }),
                        agent_id: Some(agent_id.clone()),
                        task_id: None,
                    });
                    return Err(error);
                }
            };
            let status = if outcome.success {
                "completed"
            } else {
                "failed"
            };

            let record = agent_store::build_agent_record(
                &agent_id,
                &archetype,
                &traits,
                &model,
                &prompt,
                status,
                &outcome.artifact_path,
                outcome.bytes,
                &outcome.result,
                &chrono::Utc::now().to_rfc3339(),
            );
            agent_store::persist_spawn(&record)?;

            publish_event(RuntimeEvent {
                kind: if outcome.success {
                    "agent.spawn.completed".to_string()
                } else {
                    "agent.spawn.failed".to_string()
                },
                payload: json!({
                    "agent_id": agent_id.clone(),
                    "archetype": archetype.clone(),
                    "model": model.clone(),
                    "status": status,
                    "success": outcome.success,
                    "exit_code": outcome.exit_code,
                    "artifact_path": outcome.artifact_path,
                    "artifact_bytes": outcome.bytes,
                    "selected_skills": &selected_skills,
                }),
                agent_id: Some(agent_id.clone()),
                task_id: None,
            });

            register_agent_transport(&agent_id).await;

            let stream = outcome
                .stream
                .map(|(observed, anomalies)| json!({"observed": observed, "anomalies": anomalies}));

            let structured_output = if output_schema.is_some() {
                extract_structured_output(&outcome.content)
            } else {
                None
            };

            Ok(json!({
                "agent_id": agent_id,
                "archetype": archetype,
                "status": status,
                "success": outcome.success,
                "exit_code": outcome.exit_code,
                "artifact_path": outcome.artifact_path,
                "artifact_bytes": outcome.bytes,
                "result": outcome.result,
                "selected_skills": selected_skills,
                "stream": stream,
                "structured_output": structured_output,
                "aisp": aisp_json
            }))
        })
    }
}

// ============================================================================
// agent.status
// ============================================================================

pub struct AgentStatusHandler;

impl ToolHandler for AgentStatusHandler {
    fn name(&self) -> &'static str {
        "status"
    }
    fn domain(&self) -> &'static str {
        "agent"
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "agent_id": { "type": "string", "description": "Agent UUID to query; omit to list all agents" }
            }
        })
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            if let Some(id) = params.get("agent_id").and_then(|v| v.as_str()) {
                let transport_live = TRANSPORT_REGISTRY.contains(id);
                let result = match agent_store::status_view(id, transport_live)? {
                    Some(view) => Ok(view),
                    None => Ok(json!({ "found": false, "agent_id": id })),
                };
                publish_event(RuntimeEvent {
                    kind: "agent.status.queried".to_string(),
                    payload: json!({
                        "agent_id": id,
                        "found": result.as_ref().map(|v| v["found"].as_bool().unwrap_or(false)).unwrap_or(false),
                        "transport_live": transport_live,
                    }),
                    agent_id: Some(id.to_string()),
                    task_id: None,
                });
                return result;
            }

            let agents = agent_store::list_view()?;
            publish_event(RuntimeEvent {
                kind: "agent.status.listed".to_string(),
                payload: json!({ "count": agents.len() }),
                agent_id: None,
                task_id: None,
            });
            Ok(json!({ "count": agents.len(), "agents": agents }))
        })
    }
}

// ============================================================================
// agent.message
// ============================================================================

pub struct AgentMessageHandler;

impl ToolHandler for AgentMessageHandler {
    fn name(&self) -> &'static str {
        "message"
    }
    fn domain(&self) -> &'static str {
        "agent"
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "agent_id": { "type": "string", "description": "Target agent UUID" },
                "message":  { "type": "string", "description": "Message content to deliver" }
            },
            "required": ["agent_id", "message"]
        })
    }

    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("agent_id").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'agent_id' field (string)".to_string(),
            ));
        }
        if params.get("message").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'message' field (string)".to_string(),
            ));
        }
        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let agent_id = params["agent_id"].as_str().unwrap_or_default().to_string();
            let content = params["message"].as_str().unwrap_or_default().to_string();

            let (delivered, msg_id, count) = match agent_store::append_message(&agent_id, &content)?
            {
                Some((msg_id, count)) => (true, msg_id, count),
                None => (false, String::new(), 0),
            };

            if delivered {
                transport_send(&agent_id, &content).await;
                publish_event(RuntimeEvent {
                    kind: "agent.message.delivered".to_string(),
                    payload: json!({
                        "agent_id": agent_id.clone(),
                        "message": content.clone(),
                        "message_id": msg_id.clone(),
                        "message_count": count,
                        "transport_live": TRANSPORT_REGISTRY.contains(&agent_id),
                    }),
                    agent_id: Some(agent_id.clone()),
                    task_id: None,
                });

                Ok(json!({
                    "delivered": true,
                    "agent_id": agent_id,
                    "message_id": msg_id,
                    "message_count": count,
                    "transport_live": TRANSPORT_REGISTRY.contains(&agent_id)
                }))
            } else {
                publish_event(RuntimeEvent {
                    kind: "agent.message.missed".to_string(),
                    payload: json!({
                        "agent_id": agent_id.clone(),
                        "message": content.clone(),
                    }),
                    agent_id: Some(agent_id.clone()),
                    task_id: None,
                });
                Ok(json!({
                    "delivered": false,
                    "agent_id": agent_id,
                    "error": "Agent not found"
                }))
            }
        })
    }
}
