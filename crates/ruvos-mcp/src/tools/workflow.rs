//! Workflow domain tool (1): run.
//!
//! Executes an orchestration template by really spawning the template's
//! sequence of agents (via the agent tool), each of which produces a real
//! artifact on disk. Returns the concrete per-step results — no placeholder.

use super::agent::AgentSpawnHandler;
use super::handler::{ExecuteFuture, ToolHandler};
use crate::{Result, RuvosError};
use serde_json::{json, Value};
use uuid::Uuid;

/// Known templates: workflow_type -> ordered archetype pipeline.
fn template(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "feature" => Some(&["planner", "coder", "tester", "reviewer"]),
        "bugfix" => Some(&["researcher", "coder", "tester"]),
        "refactor" => Some(&["architect", "coder", "reviewer"]),
        "security" => Some(&["security", "coder", "tester"]),
        _ => None,
    }
}

pub struct WorkflowRunHandler;

impl ToolHandler for WorkflowRunHandler {
    fn name(&self) -> &'static str {
        "run"
    }

    fn domain(&self) -> &'static str {
        "workflow"
    }

    fn validate(&self, params: &Value) -> Result<()> {
        let kind = params
            .get("workflow_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                RuvosError::InvalidParams("missing 'workflow_type' field (string)".to_string())
            })?;
        if template(kind).is_none() {
            return Err(RuvosError::InvalidParams(format!(
                "unknown workflow_type '{}'; expected feature|bugfix|refactor|security",
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
            let kind = params["workflow_type"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let task = params["task"].as_str().unwrap_or_default().to_string();
            let model = params
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("claude-haiku-4-5")
                .to_string();

            let pipeline = template(&kind).ok_or_else(|| {
                RuvosError::InvalidParams(format!("unknown workflow_type '{}'", kind))
            })?;

            let workflow_id = Uuid::new_v4().to_string();
            let spawner = AgentSpawnHandler;
            let mut steps = Vec::new();

            // Really run each archetype in order; each produces a real artifact.
            for archetype in pipeline {
                let step_prompt = format!("[{} workflow] {}", kind, task);
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
                "workflow_id": workflow_id,
                "workflow_type": kind,
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
    async fn feature_workflow_runs_full_pipeline() {
        let _g = isolate();
        let r = WorkflowRunHandler
            .execute(json!({"workflow_type": "feature", "task": "add POST /users"}))
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
                "workflow step must produce a real artifact at {}",
                path
            );
        }
    }

    #[tokio::test]
    async fn bugfix_workflow_has_three_steps() {
        let _g = isolate();
        let r = WorkflowRunHandler
            .execute(json!({"workflow_type": "bugfix", "task": "fix null deref"}))
            .await
            .unwrap();
        assert_eq!(r["step_count"], 3);
    }

    #[test]
    fn validation_rejects_unknown_template() {
        assert!(WorkflowRunHandler
            .validate(&json!({"workflow_type": "magic", "task": "x"}))
            .is_err());
        assert!(WorkflowRunHandler
            .validate(&json!({"workflow_type": "feature"}))
            .is_err());
        assert!(WorkflowRunHandler
            .validate(&json!({"workflow_type": "feature", "task": "x"}))
            .is_ok());
    }
}
