//! Orchestrate domain tool (1): run.
//!
//! Executes an orchestration template by really spawning the template's
//! sequence of agents (via the agent tool), each of which produces a real
//! artifact on disk. Returns the concrete per-step results — no placeholder.

use super::agent::AgentSpawnHandler;
use super::handler::{ExecuteFuture, ToolHandler};
use super::orchestrate_plan;
use crate::{Result, RuvosError};
use ruvos_goap::{GoapAction, GoapGoal, StateValue};
use serde_json::{json, Value};
use uuid::Uuid;

/// Known templates: template -> ordered archetype pipeline. Used only as a
/// **fallback** when GOAP planning yields nothing (it normally drives the run).
fn template(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "feature" => Some(&["planner", "coder", "tester", "reviewer"]),
        "bugfix" => Some(&["researcher", "coder", "tester"]),
        "refactor" => Some(&["architect", "coder", "tester", "reviewer"]),
        "security" => Some(&["security", "coder", "tester"]),
        // SPARC: Specification → Pseudocode → Architecture → Refinement → Completion (ADR-006).
        "sparc" => Some(&[
            "researcher",
            "planner",
            "architect",
            "coder",
            "tester",
            "reviewer",
        ]),
        _ => None,
    }
}

/// Build a `GoapGoal` from a JSON object of `{ key: bool }` desired conditions.
fn goal_from_json(name: &str, obj: &serde_json::Map<String, Value>) -> GoapGoal {
    let mut g = GoapGoal::new(name, 1.0);
    for (k, v) in obj {
        if let Some(b) = v.as_bool() {
            g = g.with_condition(k.clone(), StateValue::Bool(b));
        }
    }
    g
}

/// Build extra `GoapAction`s from a JSON `capabilities` array:
/// `[{ "name", "cost"?, "preconditions": {k:bool}, "effects": {k:bool} }]`.
fn actions_from_json(arr: &[Value]) -> Vec<GoapAction> {
    arr.iter()
        .filter_map(|c| {
            let mut a = GoapAction::new(c.get("name")?.as_str()?);
            if let Some(cost) = c.get("cost").and_then(|x| x.as_f64()) {
                a = a.with_cost(cost);
            }
            if let Some(p) = c.get("preconditions").and_then(|x| x.as_object()) {
                for (k, v) in p {
                    if let Some(b) = v.as_bool() {
                        a = a.with_precondition(k.clone(), StateValue::Bool(b));
                    }
                }
            }
            if let Some(e) = c.get("effects").and_then(|x| x.as_object()) {
                for (k, v) in e {
                    if let Some(b) = v.as_bool() {
                        a = a.with_effect(k.clone(), StateValue::Bool(b));
                    }
                }
            }
            Some(a)
        })
        .collect()
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
        // A run is driven by either a known `template` or an explicit `goal`.
        let has_goal = params
            .get("goal")
            .and_then(|g| g.as_object())
            .is_some_and(|o| !o.is_empty());
        match params.get("template").and_then(|v| v.as_str()) {
            Some(kind) => {
                if template(kind).is_none() {
                    return Err(RuvosError::InvalidParams(format!(
                        "unknown template '{}'; expected feature|bugfix|refactor|security|sparc",
                        kind
                    )));
                }
            }
            None => {
                if !has_goal {
                    return Err(RuvosError::InvalidParams(
                        "missing 'template' (string) or 'goal' (object of desired conditions)"
                            .to_string(),
                    ));
                }
            }
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
            let task = params["task"].as_str().unwrap_or_default().to_string();
            let model = params
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("claude-haiku-4-5")
                .to_string();
            let label = params
                .get("template")
                .and_then(|v| v.as_str())
                .unwrap_or("custom")
                .to_string();

            // Extra caller-supplied capabilities (actions) for the planner.
            let extra = params
                .get("capabilities")
                .and_then(|v| v.as_array())
                .map(|a| actions_from_json(a))
                .unwrap_or_default();

            // Compute the pipeline. Precedence: explicit `goal` → named `template`
            // (GOAP) → static template (fallback so behavior never regresses).
            let (pipeline, planned, plan_cost): (Vec<String>, bool, f64) = if let Some(goal_obj) =
                params.get("goal").and_then(|v| v.as_object())
            {
                let goal = goal_from_json(&label, goal_obj);
                match orchestrate_plan::plan_for_goal(&goal, &extra) {
                    Some((seq, cost)) => (seq, true, cost),
                    None => {
                        return Err(RuvosError::InvalidParams(
                            "goal is unreachable from the archetype capability library".to_string(),
                        ))
                    }
                }
            } else {
                let kind = params["template"].as_str().unwrap_or_default();
                match orchestrate_plan::plan_archetypes(kind, &extra) {
                    Some((seq, cost)) if !seq.is_empty() => (seq, true, cost),
                    _ => match template(kind) {
                        Some(stat) => (stat.iter().map(|s| s.to_string()).collect(), false, 0.0),
                        None => {
                            return Err(RuvosError::InvalidParams(format!(
                                "unknown template '{}'",
                                kind
                            )))
                        }
                    },
                }
            };

            let orchestration_id = Uuid::new_v4().to_string();
            let spawner = AgentSpawnHandler;
            let mut steps = Vec::new();
            let mut all_ok = true;

            // Really run each archetype in order; each produces a real artifact.
            // On the first failed step the pipeline stops (ADR-009): e.g. a failed
            // `tester` does not proceed to `reviewer`.
            for archetype in &pipeline {
                let step_prompt = format!("[{} orchestration] {}", label, task);
                let spawned = spawner
                    .execute(json!({
                        "archetype": archetype,
                        "prompt": step_prompt,
                        "model": model
                    }))
                    .await?;
                let success = spawned["success"].as_bool().unwrap_or(true);
                steps.push(json!({
                    "archetype": archetype,
                    "agent_id": spawned["agent_id"],
                    "status": spawned["status"],
                    "success": success,
                    "artifact_path": spawned["artifact_path"]
                }));
                if !success {
                    all_ok = false;
                    break;
                }
            }

            Ok(json!({
                "orchestration_id": orchestration_id,
                "template": label,
                "task": task,
                "status": if all_ok { "completed" } else { "failed" },
                "success": all_ok,
                "planned": planned,
                "plan_cost": plan_cost,
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

    #[tokio::test]
    async fn feature_run_is_flagged_as_planned() {
        let _g = isolate();
        let r = OrchestrateRunHandler
            .execute(json!({"template": "feature", "task": "x"}))
            .await
            .unwrap();
        assert_eq!(
            r["planned"], true,
            "named template must run through the GOAP planner"
        );
        assert!(r["plan_cost"].as_f64().unwrap() > 0.0);
    }

    #[tokio::test]
    async fn sparc_orchestration_runs_all_phases() {
        let _g = isolate();
        let r = OrchestrateRunHandler
            .execute(json!({"template": "sparc", "task": "build module"}))
            .await
            .unwrap();
        assert_eq!(r["status"], "completed");
        let steps = r["steps"].as_array().unwrap();
        let names: Vec<&str> = steps
            .iter()
            .map(|s| s["archetype"].as_str().unwrap())
            .collect();
        for phase in ["researcher", "architect", "coder", "tester", "reviewer"] {
            assert!(names.contains(&phase), "sparc missing {phase}");
        }
    }

    #[tokio::test]
    async fn custom_goal_computes_pipeline_without_template() {
        let _g = isolate();
        let r = OrchestrateRunHandler
            .execute(json!({"task": "harden auth", "goal": {"secured": true, "tested": true}}))
            .await
            .unwrap();
        assert_eq!(r["planned"], true);
        assert_eq!(r["template"], "custom");
        let names: Vec<&str> = r["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["archetype"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"security"));
        assert!(names.contains(&"tester"));
    }

    #[tokio::test]
    async fn unreachable_goal_errors() {
        let _g = isolate();
        let r = OrchestrateRunHandler
            .execute(json!({"task": "x", "goal": {"nonexistent_condition": true}}))
            .await;
        assert!(r.is_err(), "a goal no action can satisfy must error");
    }

    #[test]
    fn validation_accepts_template_or_goal() {
        // unknown template → error
        assert!(OrchestrateRunHandler
            .validate(&json!({"template": "magic", "task": "x"}))
            .is_err());
        // template without task → error
        assert!(OrchestrateRunHandler
            .validate(&json!({"template": "feature"}))
            .is_err());
        // valid template + task → ok
        assert!(OrchestrateRunHandler
            .validate(&json!({"template": "feature", "task": "x"}))
            .is_ok());
        // sparc is now a known template
        assert!(OrchestrateRunHandler
            .validate(&json!({"template": "sparc", "task": "x"}))
            .is_ok());
        // goal instead of template → ok
        assert!(OrchestrateRunHandler
            .validate(&json!({"goal": {"tested": true}, "task": "x"}))
            .is_ok());
        // neither template nor goal → error
        assert!(OrchestrateRunHandler
            .validate(&json!({"task": "x"}))
            .is_err());
    }
}
