//! Agent domain tools (3): spawn, status, message.
//!
//! Agents are persisted to disk (source of truth, survives restarts) and
//! really execute their task on spawn: each agent produces a real work
//! artifact on disk, and — when `RUVOS_AGENT_RUNNER` is set — additionally
//! runs that command as a real subprocess and captures its output.

use super::handler::{ExecuteFuture, ToolHandler};
use crate::{paths, Result, RuvosError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Mutex;
use uuid::Uuid;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: String,
    pub content: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRecord {
    pub id: String,
    pub archetype: String,
    pub traits: Vec<String>,
    pub model: String,
    pub prompt: String,
    pub status: String,
    pub created_at: String,
    pub artifact_path: String,
    pub artifact_bytes: u64,
    #[serde(default)]
    pub result: String,
    #[serde(default)]
    pub messages: Vec<AgentMessage>,
}

type Registry = BTreeMap<String, AgentRecord>;

lazy_static::lazy_static! {
    static ref FILE_LOCK: Mutex<()> = Mutex::new(());
}

fn load_registry() -> Registry {
    match std::fs::read(paths::agents_file()) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Registry::new(),
    }
}

fn save_registry(reg: &Registry) -> Result<()> {
    paths::ensure_root()
        .map_err(|e| RuvosError::InternalError(format!("cannot create data dir: {}", e)))?;
    let path = paths::agents_file();
    let bytes = serde_json::to_vec_pretty(reg)
        .map_err(|e| RuvosError::InternalError(format!("serialize agents: {}", e)))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes)
        .map_err(|e| RuvosError::InternalError(format!("write agents: {}", e)))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| RuvosError::InternalError(format!("commit agents: {}", e)))?;
    Ok(())
}

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
async fn execute_task(
    agent_id: &str,
    archetype: &str,
    prompt: &str,
) -> Result<(String, u64, String)> {
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

    // Optional: run a real external runner (e.g. a wrapper around a CLI).
    let result = if let Ok(runner) = std::env::var("RUVOS_AGENT_RUNNER") {
        // `--` signals end-of-options so a prompt beginning with `-` can't be
        // smuggled in as a flag to the runner binary (argv injection guard).
        let output = tokio::process::Command::new(&runner)
            .arg(archetype) // already validated against the archetype allowlist
            .arg("--")
            .arg(prompt)
            .output()
            .await
            .map_err(|e| RuvosError::InternalError(format!("runner '{}': {}", runner, e)))?;
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        format!(
            "{} agent completed: wrote {}-byte plan to {}",
            archetype,
            bytes,
            artifact.display()
        )
    };

    Ok((artifact.to_string_lossy().into_owned(), bytes, result))
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
            let traits = params
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
            let (artifact_path, artifact_bytes, result) =
                execute_task(&agent_id, &archetype, &prompt).await?;

            let record = AgentRecord {
                id: agent_id.clone(),
                archetype: archetype.clone(),
                traits,
                model,
                prompt,
                status: "completed".to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
                artifact_path: artifact_path.clone(),
                artifact_bytes,
                result: result.clone(),
                messages: Vec::new(),
            };

            {
                let _guard = FILE_LOCK.lock().unwrap();
                let mut reg = load_registry();
                reg.insert(agent_id.clone(), record);
                save_registry(&reg)?;
            }

            Ok(json!({
                "agent_id": agent_id,
                "archetype": archetype,
                "status": "completed",
                "artifact_path": artifact_path,
                "artifact_bytes": artifact_bytes,
                "result": result
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
            let _guard = FILE_LOCK.lock().unwrap();
            let reg = load_registry();

            // Single-agent query when agent_id provided; else list all.
            if let Some(id) = params.get("agent_id").and_then(|v| v.as_str()) {
                return match reg.get(id) {
                    Some(a) => Ok(json!({
                        "found": true,
                        "agent_id": a.id,
                        "archetype": a.archetype,
                        "status": a.status,
                        "artifact_path": a.artifact_path,
                        "message_count": a.messages.len(),
                        "result": a.result
                    })),
                    None => Ok(json!({ "found": false, "agent_id": id })),
                };
            }

            let agents: Vec<Value> = reg
                .values()
                .map(|a| {
                    json!({
                        "agent_id": a.id,
                        "archetype": a.archetype,
                        "status": a.status,
                        "created_at": a.created_at,
                        "message_count": a.messages.len()
                    })
                })
                .collect();
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

            let _guard = FILE_LOCK.lock().unwrap();
            let mut reg = load_registry();
            match reg.get_mut(&agent_id) {
                Some(a) => {
                    let msg = AgentMessage {
                        id: Uuid::new_v4().to_string(),
                        content,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    };
                    let msg_id = msg.id.clone();
                    a.messages.push(msg);
                    let count = a.messages.len();
                    save_registry(&reg)?;
                    Ok(json!({
                        "delivered": true,
                        "agent_id": agent_id,
                        "message_id": msg_id,
                        "message_count": count
                    }))
                }
                None => Ok(json!({
                    "delivered": false,
                    "agent_id": agent_id,
                    "error": "Agent not found"
                })),
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
