//! Orchestrate domain tool (1): run.
//!
//! Executes an orchestration template by really spawning the template's
//! sequence of agents (via the agent tool), each of which produces a real
//! artifact on disk. Returns the concrete per-step results — no placeholder.

use super::agent::AgentSpawnHandler;
use super::handler::{ExecuteFuture, ToolHandler};
use crate::{Result, RuvosError};
use serde_json::{json, Value};
use uuid::Uuid;

/// Known templates: template -> ordered archetype pipeline.
fn template(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "feature" => Some(&["planner", "coder", "tester", "reviewer"]),
        "bugfix" => Some(&["researcher", "coder", "tester"]),
        "refactor" => Some(&["architect", "coder", "reviewer"]),
        "security" => Some(&["security", "coder", "tester"]),
        _ => None,
    }
}

pub struct OrchestrateRunHandler;

impl ToolHandler for OrchestrateRunHandler {
    fn name(&self) -> &'static str {
        "run"
    }

    fn domain(&self) -> &'static str {
        "orchestrate"
    }

    fn validate(&self, params: &Value) -> Result<()> {
        let kind = params
            .get("template")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                RuvosError::InvalidParams("missing 'template' field (string)".to_string())
            })?;
        if template(kind).is_none() {
            return Err(RuvosError::InvalidParams(format!(
                "unknown template '{}'; expected feature|bugfix|refactor|security",
                kind
            )));
        }
        if params.get("task").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'task' field (string)".to_string(),
            ));
        }
        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let kind = params["template"].as_str().unwrap_or_default().to_string();
            let task = params["task"].as_str().unwrap_or_default().to_string();
            let model = params
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("claude-haiku-4-5")
                .to_string();

            let pipeline = template(&kind)
                .ok_or_else(|| RuvosError::InvalidParams(format!("unknown template '{}'", kind)))?;

            let orchestration_id = Uuid::new_v4().to_string();
            let spawner = AgentSpawnHandler;
            let mut steps = Vec::new();

            // Really run each archetype in order; each produces a real artifact.
            for archetype in pipeline {
                let step_prompt = format!("[{} orchestration] {}", kind, task);
                let spawned = spawner
                    .execute(json!({
                        "archetype": archetype,
                        "prompt": step_prompt,
                        "model": model
                    }))
                    .await?;
                steps.push(json!({
                    "archetype": archetype,
                    "agent_id": spawned["agent_id"],
                    "status": spawned["status"],
                    "artifact_path": spawned["artifact_path"]
                }));
            }

            Ok(json!({
                "orchestration_id": orchestration_id,
                "template": kind,
                "task": task,
                "status": "completed",
                "step_count": steps.len(),
                "steps": steps
            }))
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

    #[tokio::test]
    async fn feature_orchestration_runs_full_pipeline() {
        let _g = isolate();
        let r = OrchestrateRunHandler
            .execute(json!({"template": "feature", "task": "add POST /users"}))
            .await
            .unwrap();

        assert_eq!(r["status"], "completed");
        assert_eq!(r["step_count"], 4);
        let steps = r["steps"].as_array().unwrap();
        assert_eq!(steps[0]["archetype"], "planner");
        assert_eq!(steps[1]["archetype"], "coder");

        // Every step really produced an artifact file on disk.
        for step in steps {
            let path = step["artifact_path"].as_str().unwrap();
            assert!(
                std::path::Path::new(path).exists(),
                "orchestration step must produce a real artifact at {}",
                path
            );
        }
    }

    #[tokio::test]
    async fn bugfix_orchestration_has_three_steps() {
        let _g = isolate();
        let r = OrchestrateRunHandler
            .execute(json!({"template": "bugfix", "task": "fix null deref"}))
            .await
            .unwrap();
        assert_eq!(r["step_count"], 3);
    }

    #[test]
    fn validation_rejects_unknown_template() {
        assert!(OrchestrateRunHandler
            .validate(&json!({"template": "magic", "task": "x"}))
            .is_err());
        assert!(OrchestrateRunHandler
            .validate(&json!({"template": "feature"}))
            .is_err());
        assert!(OrchestrateRunHandler
            .validate(&json!({"template": "feature", "task": "x"}))
            .is_ok());
    }
}
