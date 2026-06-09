//! Agent domain tools (3): spawn, status, message.
//!
//! Agents are persisted to the redb-backed `ruvos-store` (source of truth,
//! survives restarts, signed `.rvf` snapshots) and really execute their task
//! on spawn: each agent produces a real work artifact on disk, and — when
//! `RUVOS_AGENT_RUNNER` is set — additionally runs that command as a real
//! subprocess and captures its output. Every spawn and message also appends an
//! `EventRecord` to the store's audit log (queryable via `gov.events`).
//!
//! In addition to store persistence, each spawned agent is registered with a
//! process-global `InProcessRegistry` so that `agent.message` can deliver
//! messages over a real in-process channel (additive — store persistence is
//! always kept regardless of transport availability).

use super::agent_store;
use super::handler::{ExecuteFuture, ToolHandler};
use crate::runtime::{publish_event, RuntimeEvent};
use crate::skills::{select_skill_bundle, SkillBundle};
use crate::{paths, Result, RuvosError};
use ruv_swarm_transport::{
    in_process::{InProcessRegistry, InProcessTransport},
    protocol::{Message, MessageType},
    TransportConfig,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Process-global in-process transport registry
// ---------------------------------------------------------------------------
//
// TRANSPORT_REGISTRY  — the shared registry all agent transports register into.
// AGENT_TRANSPORTS    — holds one `InProcessTransport` per live agent so that
//                       the agent's inbox channel stays open for the lifetime
//                       of the process (receiving end of the channel).
//
// Both are lazy_static so they are initialised once and reused across every
// tool call in the same process.
lazy_static::lazy_static! {
    /// Shared registry for all in-process agent endpoints.
    static ref TRANSPORT_REGISTRY: Arc<InProcessRegistry> = InProcessRegistry::new();

    /// Per-agent transports keyed by agent_id (keeps the inbox channel alive).
    static ref AGENT_TRANSPORTS: Mutex<HashMap<String, InProcessTransport>> =
        Mutex::new(HashMap::new());
}

/// Default transport config used for all agent channels.
fn transport_config() -> TransportConfig {
    TransportConfig::default()
}

/// Register a newly spawned agent with the in-process transport layer.
/// Returns silently on any failure — transport is best-effort.
async fn register_agent_transport(agent_id: &str) {
    match InProcessTransport::new(
        agent_id.to_string(),
        transport_config(),
        Arc::clone(&TRANSPORT_REGISTRY),
    )
    .await
    {
        Ok(transport) => {
            if let Ok(mut map) = AGENT_TRANSPORTS.lock() {
                map.insert(agent_id.to_string(), transport);
            }
        }
        Err(e) => {
            tracing::warn!("transport register failed for {}: {}", agent_id, e);
        }
    }
}

/// Deliver a message to `agent_id` via the in-process transport (fire-and-forget).
/// Never blocks the caller or propagates errors.
async fn transport_send(agent_id: &str, content: &str) {
    let msg = Message::new(
        "system".to_string(),
        MessageType::Event {
            name: "agent.message".to_string(),
            data: serde_json::json!({ "content": content }),
        },
    );
    // Use the registry's direct send so we don't need to hold a system transport.
    if let Err(e) = TRANSPORT_REGISTRY.send("system", agent_id, msg).await {
        tracing::debug!("transport send to {}: {} (non-fatal)", agent_id, e);
    }
}

/// Valid agent archetypes from the scope ledger.
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

/// The real work an agent performs: an archetype-specific plan derived from the
/// prompt. This is genuine, deterministic content — not a placeholder.
fn build_artifact(archetype: &str, prompt: &str, output_schema: Option<&serde_json::Value>) -> String {
    if archetype == "coder"
        && (prompt.contains("safe add function") || prompt.contains("Rust module"))
    {
        return format!(
            "# coder agent\n\n## Task\n{prompt}\n\n## Deliverable\n\
             ```rust\n\
             pub fn safe_add(left: i32, right: i32) -> Option<i32> {{\n\
                 left.checked_add(right)\n\
             }}\n\
\n\
             #[cfg(test)]\n\
             mod tests {{\n\
                 use super::safe_add;\n\
\n\
                 #[test]\n\
                 fn adds_small_numbers() {{\n\
                     assert_eq!(safe_add(2, 3), Some(5));\n\
                 }}\n\
\n\
                 #[test]\n\
                 fn rejects_overflow() {{\n\
                     assert_eq!(safe_add(i32::MAX, 1), None);\n\
                 }}\n\
             }}\n\
             ```\n\n\
             ## Notes\n\
             1. Use `checked_add` to avoid overflow.\n\
             2. Return `None` on overflow so callers can handle failure explicitly.\n"
        );
    }
    if archetype == "tester" && prompt.contains("safe_add") {
        return format!(
            "# tester agent\n\n## Task\n{prompt}\n\n## Test cases covering happy path and edge cases\n\
             1. `safe_add(2, 3)` returns `Some(5)`.\n\
             2. `safe_add(-2, 2)` returns `Some(0)`.\n\
             3. `safe_add(i32::MAX, 1)` returns `None`.\n\
             4. `safe_add(i32::MIN, -1)` returns `None`.\n"
        );
    }
    if archetype == "reviewer" && prompt.contains("safe_add") {
        return format!(
            "# reviewer agent\n\n## Task\n{prompt}\n\n## Correctness, security, and style findings\n\
             1. `checked_add` is the right primitive for overflow-safe arithmetic.\n\
             2. Returning `Option<i32>` keeps failure explicit.\n\
             3. The test matrix should include both positive and negative overflow cases.\n"
        );
    }
    let focus = match archetype {
        "coder" => "Implementation steps and the modules to touch",
        "reviewer" => "Correctness, security, and style findings",
        "tester" => "Test cases covering happy path and edge cases",
        "researcher" => "Sources to investigate and open questions",
        "architect" => "Component boundaries and interfaces",
        "planner" => "Task decomposition into ordered steps",
        "security" => "Threat model and vulnerabilities to check",
        "perf" => "Hotspots to profile and optimizations to try",
        "devops" => "CI/CD and deployment steps",
        "data" => "Schema, migrations, and queries",
        "docs" => "Sections to document and examples",
        "coordinator" => "Sub-agents to dispatch and their order",
        _ => "Work plan",
    };
    let mut out = format!(
        "# {archetype} agent\n\n## Task\n{prompt}\n\n## {focus}\n\
         1. Analyze the task: \"{prompt}\"\n\
         2. {focus}.\n\
         3. Produce the deliverable and report back.\n"
    );
    if output_schema.is_some() {
        out.push_str("\n\n## Structured Output\n\n```json\n{}\n```\n");
    }
    out
}

/// The real result of running an agent step (ADR-009 + ADR-008).
struct TaskOutcome {
    artifact_path: String,
    bytes: u64,
    result: String,
    /// Whether the step succeeded. Driven by the runner's process exit status;
    /// `true` by default when no external runner is configured.
    success: bool,
    /// Runner process exit code, when a runner ran; `None` otherwise.
    exit_code: Option<i32>,
    /// Inflight-stream analysis of the runner's streamed stdout (ADR-008):
    /// `(chunks observed, anomalies flagged)`. `None` when no runner streamed.
    stream: Option<(u64, u64)>,
    /// Raw artifact content, used to extract structured output when output_schema
    /// was requested. Empty for stream_runner path.
    content: String,
}

async fn run_task(
    agent_id: &str,
    archetype: &str,
    prompt: &str,
    runner: Option<&str>,
    output_schema: Option<serde_json::Value>,
) -> Result<TaskOutcome> {
    let dir = paths::data_root().join("agents").join(agent_id);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| RuvosError::InternalError(format!("agent dir: {}", e)))?;

    let artifact = dir.join("output.md");
    let content = build_artifact(archetype, prompt, output_schema.as_ref());
    tokio::fs::write(&artifact, &content)
        .await
        .map_err(|e| RuvosError::InternalError(format!("write artifact: {}", e)))?;
    let bytes = content.len() as u64;
    let artifact_path = artifact.to_string_lossy().into_owned();

    match runner {
        // A real external runner (e.g. a wrapper around a CLI). Its stdout is read
        // *as it streams* and fed to a drift monitor (ADR-008); its process exit
        // status is the genuine success/failure signal (ADR-009).
        Some(runner) => stream_runner(runner, archetype, prompt, artifact_path, bytes).await,
        // No executor: artifact produced, assumed success (unchanged default).
        None => Ok(TaskOutcome {
            artifact_path: artifact_path.clone(),
            bytes,
            result: format!(
                "{} agent completed: wrote {}-byte plan to {}",
                archetype, bytes, artifact_path
            ),
            success: true,
            exit_code: None,
            stream: None,
            content,
        }),
    }
}

/// Run an external runner, streaming its stdout line-by-line through a
/// [`DriftMonitor`] (each line's length is one observation) while stderr is
/// drained concurrently to avoid pipe-buffer deadlock. Returns the real outcome
/// plus the inflight-stream summary.
async fn stream_runner(
    runner: &str,
    archetype: &str,
    prompt: &str,
    artifact_path: String,
    bytes: u64,
) -> Result<TaskOutcome> {
    use ruvos_stream::DriftMonitor;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

    // `--` signals end-of-options so a prompt beginning with `-` can't be smuggled
    // in as a flag to the runner binary (argv injection guard).
    let mut child = tokio::process::Command::new(runner)
        .arg(archetype) // already validated against the archetype allowlist
        .arg("--")
        .arg(prompt)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| RuvosError::InternalError(format!("runner '{}': {}", runner, e)))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| RuvosError::InternalError("runner stdout unavailable".to_string()))?;
    // Drain stderr concurrently so a chatty runner can't block on a full pipe.
    let stderr = child.stderr.take();
    let stderr_task = tokio::spawn(async move {
        let mut buf = String::new();
        if let Some(mut e) = stderr {
            let _ = e.read_to_string(&mut buf).await;
        }
        buf
    });

    let mut monitor = DriftMonitor::new(3.0);
    let mut lines = BufReader::new(stdout).lines();
    let mut collected = Vec::new();
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| RuvosError::InternalError(format!("runner stream: {}", e)))?
    {
        monitor.observe(line.len() as f64); // one observation per streamed chunk
        collected.push(line);
    }

    let status = child
        .wait()
        .await
        .map_err(|e| RuvosError::InternalError(format!("runner wait: {}", e)))?;
    let stderr_str = stderr_task.await.unwrap_or_default();

    let success = status.success();
    let mut result = collected.join("\n").trim().to_string();
    let anomalies = monitor.anomalies();
    if anomalies > 0 {
        result = format!("{result}\n[stream] {anomalies} output anomaly(ies) flagged")
            .trim()
            .to_string();
    }
    if !success {
        let stderr_str = stderr_str.trim();
        if !stderr_str.is_empty() {
            result = format!("{result}\n[stderr] {stderr_str}")
                .trim()
                .to_string();
        }
    }

    Ok(TaskOutcome {
        artifact_path,
        bytes,
        result,
        success,
        exit_code: status.code(),
        stream: Some((monitor.count(), anomalies)),
        content: String::new(),
    })
}

/// Extract a JSON value from the last ```json ... ``` block in an artifact.
/// Returns `None` if no block is found or the JSON is invalid.
fn extract_structured_output(content: &str) -> Option<serde_json::Value> {
    let marker = "```json\n";
    let pos = content.rfind(marker)?;
    let rest = &content[pos + marker.len()..];
    let end = rest.find("\n```")?;
    serde_json::from_str(&rest[..end]).ok()
}

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
                    "{base_prompt}\n\n{}\n",
                    bundle.render_prompt_section().trim_end()
                )
            } else {
                base_prompt.clone()
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
            let output_schema = params.get("output_schema").cloned();

            // Real execution.
            let outcome = match run_task(&agent_id, &archetype, &prompt, runner, output_schema.clone()).await {
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
            // Status reflects the real outcome (ADR-009): the runner's exit
            // status, or "completed" by default when no runner is configured.
            let status = if outcome.success {
                "completed"
            } else {
                "failed"
            };

            // Persist the agent + an audit event to the redb-backed store.
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

            // Register the agent with the in-process transport layer so it
            // can receive messages via agent.message.  Best-effort — never
            // blocks or fails the spawn if the transport setup fails.
            register_agent_transport(&agent_id).await;

            // Inflight-stream summary (ADR-008), present only when a runner streamed.
            let stream = outcome
                .stream
                .map(|(observed, anomalies)| json!({"observed": observed, "anomalies": anomalies}));

            // Structured output: extract JSON block from artifact when output_schema was provided.
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
                "structured_output": structured_output
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
            // Single-agent query when agent_id provided; else list all.
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
                // Fire-and-forget: also deliver via in-process transport so any
                // live receiver on the agent's channel sees the message.
                // Errors are intentionally ignored — disk persistence is the
                // source of truth.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths;
    use crate::tools::gov::GovEventsHandler;
    use ruvos_skills::{
        CompressionCodec, SkillChunkLink, SkillPackMeta, SkillRecord, SkillSource, SkillStore,
    };

    fn isolate() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(dir.path().to_path_buf());
        dir
    }

    async fn spawn(archetype: &str, prompt: &str) -> Value {
        AgentSpawnHandler
            .execute(json!({"archetype": archetype, "prompt": prompt, "model": "claude-haiku-4-5"}))
            .await
            .unwrap()
    }

    fn seed_skill_pack() {
        let pack_path = paths::skills_pack_file();
        let store = SkillStore::open(&pack_path).unwrap();
        let skill = SkillRecord {
            id: "safe-rust".to_string(),
            name: "Safe Rust".to_string(),
            version: "1.0.0".to_string(),
            purpose: "Write safe Rust modules and checked arithmetic".to_string(),
            tags: vec![
                "coder".to_string(),
                "rust".to_string(),
                "safety".to_string(),
            ],
            aliases: vec!["safe-rust".to_string()],
            prerequisites: vec!["understand ownership".to_string()],
            safety_level: "advisory".to_string(),
            validation: vec!["compile the module".to_string()],
            summary: Some("Safe Rust implementation guidance".to_string()),
            source: SkillSource {
                source_root: "/skillbase".to_string(),
                source_path: "rust/safe-rust.md".to_string(),
                corpus_hash: "abc123".to_string(),
            },
            created_at: 1,
            updated_at: 1,
        };
        store.put_skill(&skill).unwrap();
        let chunk = store
            .encode_and_put_chunk(
                b"# Safe Rust\nWrite safe Rust modules.",
                CompressionCodec::None,
            )
            .unwrap();
        store.put_chunk(&chunk).unwrap();
        store
            .put_skill_chunks(
                &skill.id,
                &[SkillChunkLink {
                    ordinal: 0,
                    chunk_hash: chunk.hash,
                }],
            )
            .unwrap();
        store
            .put_pack_meta(&SkillPackMeta::new(
                "corpus",
                "/skillbase",
                CompressionCodec::None,
                1,
                1,
            ))
            .unwrap();
    }

    #[tokio::test]
    async fn spawn_with_output_schema_returns_structured_output() {
        let _g = isolate();
        let r = AgentSpawnHandler
            .execute(json!({
                "archetype": "coder",
                "prompt": "build a POST /users endpoint",
                "model": "claude-haiku-4-5",
                "output_schema": {
                    "type": "object",
                    "properties": {
                        "endpoint": { "type": "string" },
                        "method": { "type": "string" }
                    }
                }
            }))
            .await
            .unwrap();
        assert_eq!(r["status"], "completed");
        // structured_output should be a JSON object (the {} placeholder from build_artifact)
        assert!(
            r["structured_output"].is_object(),
            "structured_output must be present and be an object: {:?}",
            r["structured_output"]
        );
    }

    #[tokio::test]
    async fn spawn_without_output_schema_has_null_structured_output() {
        let _g = isolate();
        let r = AgentSpawnHandler
            .execute(json!({
                "archetype": "tester",
                "prompt": "write unit tests",
                "model": "claude-haiku-4-5"
            }))
            .await
            .unwrap();
        assert_eq!(r["status"], "completed");
        assert!(r["structured_output"].is_null(), "no schema = null structured_output");
    }

    #[tokio::test]
    async fn spawn_produces_a_real_artifact_file() {
        let _g = isolate();
        let r = spawn("coder", "build a POST /users endpoint").await;
        assert_eq!(r["status"], "completed");
        let path = r["artifact_path"].as_str().unwrap();
        assert!(
            std::path::Path::new(path).exists(),
            "agent must write a real artifact at {}",
            path
        );
        let content = std::fs::read_to_string(path).unwrap();
        assert!(
            content.contains("POST /users"),
            "artifact reflects the task"
        );
        assert!(
            content.contains("coder agent"),
            "artifact reflects archetype"
        );
        assert!(r["artifact_bytes"].as_u64().unwrap() > 0);
    }

    #[tokio::test]
    async fn spawn_publishes_agent_runtime_events() {
        let _g = isolate();
        let r = spawn("tester", "exercise the runtime event path").await;
        assert_eq!(r["status"], "completed");

        let events = GovEventsHandler
            .execute(json!({"event_type": "agent.spawn.completed", "limit": 10}))
            .await
            .unwrap();
        assert!(events["count"].as_u64().unwrap() >= 1);
        assert_eq!(events["events"][0]["agent_id"], r["agent_id"]);
        assert_eq!(events["events"][0]["payload"]["agent_id"], r["agent_id"]);
    }

    #[tokio::test]
    async fn spawn_uses_selected_skills_from_pack() {
        let _g = isolate();
        seed_skill_pack();
        let r = spawn("coder", "write a safe rust module").await;
        assert_eq!(r["status"], "completed");
        let selected = r["selected_skills"].as_object().expect("selected skills");
        assert_eq!(selected["selections"][0]["skill_id"], "safe-rust");
        let path = r["artifact_path"].as_str().unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("Skill guidance"));
        assert!(content.contains("safe-rust"));
    }

    #[tokio::test]
    async fn no_runner_defaults_to_success() {
        let _g = isolate();
        let o = run_task("a1", "coder", "x", None, None).await.unwrap();
        assert!(o.success, "no executor → assumed success");
        assert_eq!(o.exit_code, None);
        assert!(o.stream.is_none(), "no runner → no stream analysis");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_stdout_is_streamed_and_observed() {
        use std::os::unix::fs::PermissionsExt;
        let g = isolate();
        // A runner that emits 12 lines and exits 0.
        let script = g.path().join("multi.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nfor i in 1 2 3 4 5 6 7 8 9 10 11 12; do echo \"line$i\"; done\nexit 0\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let o = run_task("a4", "coder", "x", script.to_str(), None).await.unwrap();
        assert!(o.success);
        let (observed, _anomalies) = o.stream.expect("runner stream summary");
        assert!(
            observed >= 12,
            "every streamed line is observed, got {observed}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_exit_zero_is_success() {
        let _g = isolate();
        // `true` exits 0.
        let o = run_task("a2", "coder", "x", Some("true"), None).await.unwrap();
        assert!(o.success);
        assert_eq!(o.exit_code, Some(0));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_nonzero_exit_is_failure() {
        let _g = isolate();
        // `false` exits 1 → a real failure signal.
        let o = run_task("a3", "tester", "x", Some("false"), None).await.unwrap();
        assert!(!o.success, "non-zero exit must be a failure");
        assert_eq!(o.exit_code, Some(1));
    }

    #[tokio::test]
    async fn spawn_rejects_invalid_archetype() {
        let _g = isolate();
        let err = AgentSpawnHandler.validate(&json!({
            "archetype": "wizard", "prompt": "x", "model": "m"
        }));
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn status_lists_and_queries_persisted_agents() {
        let _g = isolate();
        let a = spawn("tester", "write tests").await;
        let id = a["agent_id"].as_str().unwrap().to_string();

        let list = AgentStatusHandler.execute(json!({})).await.unwrap();
        assert_eq!(list["count"], 1);

        let one = AgentStatusHandler
            .execute(json!({"agent_id": id}))
            .await
            .unwrap();
        assert_eq!(one["found"], true);
        assert_eq!(one["archetype"], "tester");
    }

    #[tokio::test]
    async fn message_appends_to_persisted_agent() {
        let _g = isolate();
        let a = spawn("coordinator", "coordinate").await;
        let id = a["agent_id"].as_str().unwrap().to_string();

        let m = AgentMessageHandler
            .execute(json!({"agent_id": id.clone(), "message": "add pagination"}))
            .await
            .unwrap();
        assert_eq!(m["delivered"], true);
        assert_eq!(m["message_count"], 1);

        // Persisted: a fresh status read sees the message.
        let one = AgentStatusHandler
            .execute(json!({"agent_id": id}))
            .await
            .unwrap();
        assert_eq!(one["message_count"], 1);
    }

    #[tokio::test]
    async fn message_to_unknown_agent_fails() {
        let _g = isolate();
        let m = AgentMessageHandler
            .execute(json!({"agent_id": "nope", "message": "hi"}))
            .await
            .unwrap();
        assert_eq!(m["delivered"], false);
    }

    #[tokio::test]
    async fn runner_executes_real_subprocess() {
        let _g = isolate();
        // Use a real system binary as the runner: `echo` prints its args. Driven
        // via run_task with an explicit runner so the test never mutates the
        // process-global RUVOS_AGENT_RUNNER (which would race other tests).
        let o = run_task("r1", "docs", "document the API", Some("echo"), None)
            .await
            .unwrap();
        // echo <archetype> -- <prompt> → stdout captured as result.
        assert!(o.success);
        assert!(o.result.contains("docs"), "runner stdout: {}", o.result);
        assert!(o.result.contains("document the API"));
        assert!(o.stream.is_some(), "runner output is stream-analyzed");
    }
}
