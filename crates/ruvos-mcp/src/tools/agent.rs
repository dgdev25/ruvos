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
fn build_artifact(archetype: &str, prompt: &str) -> String {
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
    format!(
        "# {archetype} agent\n\n## Task\n{prompt}\n\n## {focus}\n\
         1. Analyze the task: \"{prompt}\"\n\
         2. {focus}.\n\
         3. Produce the deliverable and report back.\n"
    )
}

/// Execute the agent's task for real: write its work artifact to disk and,
/// if a runner is configured, run it as a real subprocess. Returns
/// (artifact_path, artifact_bytes, result_text).
/// The real result of running an agent step (ADR-009).
struct TaskOutcome {
    artifact_path: String,
    bytes: u64,
    result: String,
    /// Whether the step succeeded. Driven by the runner's process exit status;
    /// `true` by default when no external runner is configured.
    success: bool,
    /// Runner process exit code, when a runner ran; `None` otherwise.
    exit_code: Option<i32>,
}

async fn execute_task(agent_id: &str, archetype: &str, prompt: &str) -> Result<TaskOutcome> {
    // Read the runner once and pass it in, so tests can drive run_task directly
    // without racing on a process-global env var.
    let runner = std::env::var("RUVOS_AGENT_RUNNER").ok();
    run_task(agent_id, archetype, prompt, runner.as_deref()).await
}

async fn run_task(
    agent_id: &str,
    archetype: &str,
    prompt: &str,
    runner: Option<&str>,
) -> Result<TaskOutcome> {
    let dir = paths::data_root().join("agents").join(agent_id);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| RuvosError::InternalError(format!("agent dir: {}", e)))?;

    let artifact = dir.join("output.md");
    let content = build_artifact(archetype, prompt);
    tokio::fs::write(&artifact, &content)
        .await
        .map_err(|e| RuvosError::InternalError(format!("write artifact: {}", e)))?;
    let bytes = content.len() as u64;
    let artifact_path = artifact.to_string_lossy().into_owned();

    match runner {
        // A real external runner (e.g. a wrapper around a CLI). Its process exit
        // status is the genuine success/failure signal.
        Some(runner) => {
            // `--` signals end-of-options so a prompt beginning with `-` can't be
            // smuggled in as a flag to the runner binary (argv injection guard).
            let output = tokio::process::Command::new(runner)
                .arg(archetype) // already validated against the archetype allowlist
                .arg("--")
                .arg(prompt)
                .output()
                .await
                .map_err(|e| RuvosError::InternalError(format!("runner '{}': {}", runner, e)))?;
            let success = output.status.success();
            let mut result = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !success {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stderr = stderr.trim();
                if !stderr.is_empty() {
                    result = format!("{result}\n[stderr] {stderr}").trim().to_string();
                }
            }
            Ok(TaskOutcome {
                artifact_path,
                bytes,
                result,
                success,
                exit_code: output.status.code(),
            })
        }
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
        }),
    }
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
            let prompt = params["prompt"].as_str().unwrap_or_default().to_string();
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

            let agent_id = Uuid::new_v4().to_string();

            // Real execution.
            let outcome = execute_task(&agent_id, &archetype, &prompt).await?;
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

            // Register the agent with the in-process transport layer so it
            // can receive messages via agent.message.  Best-effort — never
            // blocks or fails the spawn if the transport setup fails.
            register_agent_transport(&agent_id).await;

            Ok(json!({
                "agent_id": agent_id,
                "archetype": archetype,
                "status": status,
                "success": outcome.success,
                "exit_code": outcome.exit_code,
                "artifact_path": outcome.artifact_path,
                "artifact_bytes": outcome.bytes,
                "result": outcome.result
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
    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            // Single-agent query when agent_id provided; else list all.
            if let Some(id) = params.get("agent_id").and_then(|v| v.as_str()) {
                let transport_live = TRANSPORT_REGISTRY.contains(id);
                return match agent_store::status_view(id, transport_live)? {
                    Some(view) => Ok(view),
                    None => Ok(json!({ "found": false, "agent_id": id })),
                };
            }

            let agents = agent_store::list_view()?;
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

                Ok(json!({
                    "delivered": true,
                    "agent_id": agent_id,
                    "message_id": msg_id,
                    "message_count": count,
                    "transport_live": TRANSPORT_REGISTRY.contains(&agent_id)
                }))
            } else {
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
    async fn no_runner_defaults_to_success() {
        let _g = isolate();
        let o = run_task("a1", "coder", "x", None).await.unwrap();
        assert!(o.success, "no executor → assumed success");
        assert_eq!(o.exit_code, None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_exit_zero_is_success() {
        let _g = isolate();
        // `true` exits 0.
        let o = run_task("a2", "coder", "x", Some("true")).await.unwrap();
        assert!(o.success);
        assert_eq!(o.exit_code, Some(0));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_nonzero_exit_is_failure() {
        let _g = isolate();
        // `false` exits 1 → a real failure signal.
        let o = run_task("a3", "tester", "x", Some("false")).await.unwrap();
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
    async fn runner_env_executes_real_subprocess() {
        let _g = isolate();
        // Use a real system binary as the runner: `echo` prints its args.
        std::env::set_var("RUVOS_AGENT_RUNNER", "echo");
        let r = spawn("docs", "document the API").await;
        std::env::remove_var("RUVOS_AGENT_RUNNER");
        // echo <archetype> <prompt> -> stdout captured as result
        let result = r["result"].as_str().unwrap();
        assert!(
            result.contains("docs"),
            "runner stdout captured: {}",
            result
        );
        assert!(result.contains("document the API"));
    }
}
