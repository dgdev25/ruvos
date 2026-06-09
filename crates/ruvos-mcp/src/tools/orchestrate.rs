//! Orchestrate domain tool (1): run.
//!
//! Executes an orchestration template by really spawning the template's
//! sequence of agents (via the agent tool), each of which produces a real
//! artifact on disk. Returns the concrete per-step results — no placeholder.

use super::agent::AgentSpawnHandler;
use super::handler::{ExecuteFuture, ToolHandler};
use super::orchestrate_plan;
use crate::runtime::{
    classify_failure, publish_event, repair_action_for, FailureClass, RuntimeEvent,
};
use crate::skills::{record_skill_bundle_feedback, select_orchestration_skill_bundle, SkillBundle};
use crate::swarm;
use crate::{Result, RuvosError};
use ruvos_goap::{GoapAction, GoapGoal, StateValue};
use ruvos_graphflow::{EdgeCond, FlowGraph};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
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

/// Compile a linear archetype plan into a conditional-edge graph (ADR-007):
/// forward `OnSuccess` edges, plus an `OnFailure` edge from each step back to the
/// nearest preceding `coder` (rework), or to itself (retry) when none precedes.
/// With `max_retries == 0` this graph is unused — the plain stop-on-failure loop runs.
fn build_graph(pipeline: &[String]) -> FlowGraph {
    let mut g = FlowGraph::new(pipeline[0].clone());
    for i in 0..pipeline.len() {
        if i + 1 < pipeline.len() {
            g = g.edge(
                pipeline[i].clone(),
                pipeline[i + 1].clone(),
                EdgeCond::OnSuccess,
            );
        }
        let rework = pipeline[..i]
            .iter()
            .rposition(|a| a == "coder")
            .map(|p| pipeline[p].clone())
            .unwrap_or_else(|| pipeline[i].clone());
        g = g.edge(pipeline[i].clone(), rework, EdgeCond::OnFailure);
    }
    g
}

/// Spawn one archetype step; return `(success, step-json)`.
struct StepContext<'a> {
    label: &'a str,
    task: &'a str,
    model: &'a str,
    archetype: &'a str,
    swarm_role: Option<&'a str>,
    runner: Option<&'a str>,
    context: Option<&'a str>,
    skill_bundle: Option<&'a SkillBundle>,
}

async fn run_step(ctx: StepContext<'_>) -> Result<(bool, Value)> {
    let StepContext {
        label,
        task,
        model,
        archetype,
        swarm_role,
        runner,
        context,
        skill_bundle,
    } = ctx;
    let role_prefix = swarm_role
        .map(|role| format!("[{} orchestration as {}] {}", label, role, task))
        .unwrap_or_else(|| format!("[{} orchestration] {}", label, task));
    let prompt = if let Some(context) = context {
        format!("{role_prefix}\n\nPrevious artifact to consume:\n{context}")
    } else {
        role_prefix
    };
    let spawned = AgentSpawnHandler
        .execute(json!({
            "archetype": archetype,
            "prompt": prompt,
            "model": model,
            "runner": runner,
            "skill_bundle": skill_bundle,
        }))
        .await?;
    let success = spawned["success"].as_bool().unwrap_or(true);
    let step = json!({
        "archetype": archetype,
        "swarm_role": swarm_role,
        "agent_id": spawned["agent_id"],
        "status": spawned["status"],
        "success": success,
        "artifact_path": spawned["artifact_path"],
        "selected_skills": spawned["selected_skills"]
    });
    Ok((success, step))
}

fn failure_detail(success: bool, step: &Value) -> String {
    if success {
        return String::new();
    }
    let exit_code = step.get("exit_code").and_then(|value| value.as_i64());
    match exit_code {
        Some(code) => format!("step exited with code {code}"),
        None => step
            .get("status")
            .and_then(|value| value.as_str())
            .map(|status| format!("step status {status}"))
            .unwrap_or_else(|| "step failed".to_string()),
    }
}

fn emit_repair_events(
    orchestration_id: &str,
    step_index: usize,
    archetype: &str,
    detail: &str,
    retries_remaining: usize,
) -> FailureClass {
    let failure_class = classify_failure(detail);
    let repair_action = repair_action_for(failure_class, retries_remaining);
    publish_event(RuntimeEvent {
        kind: "repair.classified".to_string(),
        payload: json!({
            "orchestration_id": orchestration_id,
            "step_index": step_index,
            "archetype": archetype,
            "detail": detail,
            "failure_class": format!("{:?}", failure_class),
        }),
        agent_id: None,
        task_id: Some(orchestration_id.to_string()),
    });
    publish_event(RuntimeEvent {
        kind: "repair.plan.generated".to_string(),
        payload: json!({
            "orchestration_id": orchestration_id,
            "step_index": step_index,
            "archetype": archetype,
            "detail": detail,
            "failure_class": format!("{:?}", failure_class),
            "repair_action": format!("{:?}", repair_action),
            "retries_remaining": retries_remaining,
        }),
        agent_id: None,
        task_id: Some(orchestration_id.to_string()),
    });
    failure_class
}

fn extract_rust_source(markdown: &str) -> Option<String> {
    let start = markdown.find("```rust")?;
    let body = &markdown[start + "```rust".len()..];
    let end = body.find("```")?;
    let source = body[..end].trim();
    if source.is_empty() {
        None
    } else {
        Some(format!("{source}\n"))
    }
}

fn write_generated_source(
    orchestration_id: &str,
    step_index: usize,
    archetype: &str,
    artifact_path: &str,
) -> Option<String> {
    let markdown = fs::read_to_string(artifact_path).ok()?;
    let source = extract_rust_source(&markdown)?;
    let out_dir = PathBuf::from("generated").join(orchestration_id);
    if fs::create_dir_all(&out_dir).is_err() {
        return None;
    }
    let file_name = format!("{step_index:02}-{archetype}.rs");
    let out_path = out_dir.join(file_name);
    if fs::write(&out_path, source).is_ok() {
        Some(out_path.to_string_lossy().into_owned())
    } else {
        None
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

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "template": {
                    "type": "string",
                    "enum": ["feature", "bugfix", "refactor", "security", "sparc"],
                    "description": "Named orchestration template (pipeline of archetypes)"
                },
                "goal": {
                    "type": "object",
                    "description": "GOAP goal object specifying desired end conditions",
                    "additionalProperties": true
                },
                "task": {
                    "type": "string",
                    "description": "Human-readable task description passed to every agent in the pipeline"
                },
                "model": {
                    "type": "string",
                    "description": "Optional model override for all pipeline agents"
                }
            },
            "required": ["task"]
        })
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
            let runner = params.get("runner").and_then(|v| v.as_str());
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
            // Optional bounded retry/rework (ADR-007). 0 (default) = the plain
            // stop-on-failure pipeline; >0 routes through the conditional-edge
            // graph so a failed step loops back to the nearest coder, bounded.
            let max_retries = params
                .get("max_retries")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;
            let mut selection_hints = Vec::new();
            if max_retries > 0 {
                selection_hints.push(format!("retries={max_retries}"));
            }
            if let Some(goal_obj) = params.get("goal").and_then(|v| v.as_object()) {
                selection_hints.extend(goal_obj.keys().cloned());
            }
            selection_hints.extend(extra.iter().map(|capability| capability.name.clone()));

            let swarm_plan = swarm::recommend_plan(
                &json!({
                    "objective": task.clone(),
                    "task": task.clone(),
                    "goal": params.get("goal").cloned(),
                    "template": label.clone(),
                    "members": pipeline.iter().map(|archetype| {
                        json!({
                            "agent_id": archetype,
                            "role": archetype,
                        })
                    }).collect::<Vec<_>>(),
                    "max_agents": pipeline.len().max(1) as u32,
                }),
                pipeline.len(),
                pipeline.len().max(1) as u32,
            );
            let swarm_roles: Vec<String> = swarm_plan
                .get("default_roles")
                .and_then(|value| value.as_array())
                .map(|roles| {
                    roles
                        .iter()
                        .filter_map(|role| role.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let swarm_phases: Vec<String> = swarm_plan
                .get("phases")
                .and_then(|value| value.as_array())
                .map(|phases| {
                    phases
                        .iter()
                        .filter_map(|phase| phase.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            selection_hints.extend(swarm_roles.clone());
            selection_hints.extend(swarm_phases.clone());

            let task_skill_bundle = match select_orchestration_skill_bundle(
                &label,
                &task,
                &pipeline,
                &selection_hints,
                5,
            ) {
                Ok(bundle) => bundle,
                Err(error) => {
                    tracing::debug!("task skill selection unavailable for {}: {}", label, error);
                    None
                }
            };
            let selected_skill_bundle_path = task_skill_bundle
                .as_ref()
                .and_then(|bundle| bundle.persist_to_disk(&orchestration_id).ok());

            let mut steps = Vec::new();
            let mut generated_sources = Vec::new();
            let mut all_ok = true;
            let mut previous_artifact: Option<String> = None;
            let swarm_step_roles: Vec<String> = pipeline
                .iter()
                .enumerate()
                .map(|(index, archetype)| {
                    swarm_roles
                        .get(index)
                        .cloned()
                        .unwrap_or_else(|| archetype.clone())
                })
                .collect();
            publish_event(RuntimeEvent {
                kind: "orchestrate.run.started".to_string(),
                payload: json!({
                    "orchestration_id": orchestration_id.clone(),
                    "template": label.clone(),
                    "task": task.clone(),
                    "planned": planned,
                    "plan_cost": plan_cost,
                    "max_retries": max_retries,
                    "swarm_plan": &swarm_plan,
                    "swarm_roles": &swarm_step_roles,
                    "selected_skills": &task_skill_bundle,
                    "selected_skill_bundle_path": &selected_skill_bundle_path,
                }),
                agent_id: None,
                task_id: Some(orchestration_id.clone()),
            });

            if max_retries == 0 {
                // Linear: run each archetype in order; stop at the first failure
                // (ADR-009) — a failed `tester` does not proceed to `reviewer`.
                for (step_index, archetype) in pipeline.iter().enumerate() {
                    let (success, step) = run_step(StepContext {
                        label: &label,
                        task: &task,
                        model: &model,
                        archetype,
                        swarm_role: swarm_step_roles.get(step_index).map(String::as_str),
                        runner,
                        context: previous_artifact.as_deref(),
                        skill_bundle: task_skill_bundle.as_ref(),
                    })
                    .await?;
                    publish_event(RuntimeEvent {
                        kind: "orchestrate.step.completed".to_string(),
                        payload: json!({
                            "orchestration_id": orchestration_id.clone(),
                            "step_index": step_index,
                            "archetype": archetype.clone(),
                            "success": success,
                            "artifact_path": step["artifact_path"],
                            "agent_id": step["agent_id"],
                            "swarm_role": step["swarm_role"],
                            "selected_skills": step["selected_skills"],
                        }),
                        agent_id: step["agent_id"].as_str().map(|s| s.to_string()),
                        task_id: Some(orchestration_id.clone()),
                    });
                    steps.push(step);
                    if let Some(artifact_path) =
                        steps.last().and_then(|step| step["artifact_path"].as_str())
                    {
                        if let Some(source_path) = write_generated_source(
                            &orchestration_id,
                            step_index,
                            archetype,
                            artifact_path,
                        ) {
                            generated_sources.push(json!({
                                "archetype": archetype,
                                "source_path": source_path,
                            }));
                        }
                    }
                    previous_artifact = steps
                        .last()
                        .and_then(|step| step["artifact_path"].as_str())
                        .and_then(|path| std::fs::read_to_string(path).ok());
                    if !success {
                        all_ok = false;
                        let detail = failure_detail(success, steps.last().unwrap());
                        emit_repair_events(&orchestration_id, step_index, archetype, &detail, 0);
                        break;
                    }
                }
            } else {
                // Graph-driven: follow conditional edges with observable
                // transitions, keeping the concrete step artifacts in `steps`.
                let graph = build_graph(&pipeline);
                let max_visits = max_retries + 1;
                let max_steps = pipeline.len() * (max_retries + 1) + 2;
                let mut visits: HashMap<String, usize> = HashMap::new();
                let mut current = graph.start().to_string();
                all_ok = false;
                let mut step_index = 0usize;
                let mut previous_artifact: Option<String> = None;
                publish_event(RuntimeEvent {
                    kind: "graphflow.run.started".to_string(),
                    payload: json!({
                        "orchestration_id": orchestration_id.clone(),
                        "start": current.clone(),
                        "max_visits": max_visits,
                        "max_steps": max_steps,
                    }),
                    agent_id: None,
                    task_id: Some(orchestration_id.clone()),
                });
                for _ in 0..max_steps {
                    *visits.entry(current.clone()).or_insert(0) += 1;
                    let (success, step) = run_step(StepContext {
                        label: &label,
                        task: &task,
                        model: &model,
                        archetype: &current,
                        swarm_role: swarm_step_roles.get(step_index).map(String::as_str),
                        runner,
                        context: previous_artifact.as_deref(),
                        skill_bundle: task_skill_bundle.as_ref(),
                    })
                    .await?;
                    let next = graph.next(&current, success).map(|node| node.to_string());
                    publish_event(RuntimeEvent {
                        kind: "graphflow.step".to_string(),
                        payload: json!({
                            "orchestration_id": orchestration_id.clone(),
                            "node": current.clone(),
                            "success": success,
                            "next": next.clone(),
                        }),
                        agent_id: step["agent_id"].as_str().map(|s| s.to_string()),
                        task_id: Some(orchestration_id.clone()),
                    });
                    publish_event(RuntimeEvent {
                        kind: "orchestrate.step.completed".to_string(),
                        payload: json!({
                            "orchestration_id": orchestration_id.clone(),
                            "step_index": step_index,
                            "archetype": current.clone(),
                            "success": success,
                            "artifact_path": step["artifact_path"],
                            "agent_id": step["agent_id"],
                            "swarm_role": step["swarm_role"],
                            "selected_skills": step["selected_skills"],
                        }),
                        agent_id: step["agent_id"].as_str().map(|s| s.to_string()),
                        task_id: Some(orchestration_id.clone()),
                    });
                    step_index += 1;
                    steps.push(step);
                    if let Some(artifact_path) =
                        steps.last().and_then(|step| step["artifact_path"].as_str())
                    {
                        if let Some(source_path) = write_generated_source(
                            &orchestration_id,
                            step_index.saturating_sub(1),
                            &current,
                            artifact_path,
                        ) {
                            generated_sources.push(json!({
                                "archetype": current.clone(),
                                "source_path": source_path,
                            }));
                        }
                    }
                    previous_artifact = steps
                        .last()
                        .and_then(|step| step["artifact_path"].as_str())
                        .and_then(|path| std::fs::read_to_string(path).ok());
                    if !success {
                        let detail = failure_detail(success, steps.last().unwrap());
                        emit_repair_events(
                            &orchestration_id,
                            step_index.saturating_sub(1),
                            &current,
                            &detail,
                            max_retries.saturating_sub(*visits.get(&current).unwrap_or(&1)),
                        );
                    }
                    match next {
                        None => {
                            all_ok = success;
                            publish_event(RuntimeEvent {
                                kind: if success {
                                    "graphflow.run.completed".to_string()
                                } else {
                                    "graphflow.run.failed".to_string()
                                },
                                payload: json!({
                                    "orchestration_id": orchestration_id.clone(),
                                    "node": current.clone(),
                                    "success": success,
                                }),
                                agent_id: None,
                                task_id: Some(orchestration_id.clone()),
                            });
                            break;
                        }
                        Some(next_node) => {
                            if visits.get(&next_node).copied().unwrap_or(0) >= max_visits {
                                publish_event(RuntimeEvent {
                                    kind: "graphflow.run.visit_cap_exceeded".to_string(),
                                    payload: json!({
                                        "orchestration_id": orchestration_id.clone(),
                                        "node": next_node,
                                        "max_visits": max_visits,
                                    }),
                                    agent_id: None,
                                    task_id: Some(orchestration_id.clone()),
                                });
                                break;
                            }
                            current = next_node;
                        }
                    }
                }
            }

            publish_event(RuntimeEvent {
                kind: if all_ok {
                    "orchestrate.run.completed".to_string()
                } else {
                    "orchestrate.run.failed".to_string()
                },
                payload: json!({
                    "orchestration_id": orchestration_id.clone(),
                    "template": label.clone(),
                    "task": task.clone(),
                    "step_count": steps.len(),
                    "success": all_ok,
                    "swarm_plan": &swarm_plan,
                    "swarm_roles": &swarm_step_roles,
                    "selected_skills": &task_skill_bundle,
                    "selected_skill_bundle_path": &selected_skill_bundle_path,
                }),
                agent_id: None,
                task_id: Some(orchestration_id.clone()),
            });

            if let Some(bundle) = &task_skill_bundle {
                let outcome_label = if all_ok { "completed" } else { "failed" };
                if let Err(error) = record_skill_bundle_feedback(
                    bundle,
                    all_ok,
                    outcome_label,
                    Some(format!("orchestration_id={orchestration_id}")),
                ) {
                    tracing::debug!(
                        "skill feedback recording failed for orchestration {}: {}",
                        orchestration_id,
                        error
                    );
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
                "swarm_plan": swarm_plan,
                "swarm_roles": swarm_step_roles,
                "step_count": steps.len(),
                "generated_sources": generated_sources,
                "selected_skills": &task_skill_bundle,
                "selected_skill_bundle_path": &selected_skill_bundle_path,
                "steps": steps
            }))
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
        drop(store);
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
        assert!(r["swarm_plan"].is_object());
        assert!(r["swarm_plan"]["phases"].is_array());
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
    async fn orchestration_uses_selected_skills_from_pack() {
        let _g = isolate();
        seed_skill_pack();
        let r = OrchestrateRunHandler
            .execute(json!({
                "template": "feature",
                "task": "write a safe rust module with checked arithmetic"
            }))
            .await
            .unwrap();
        let selected = r["selected_skills"].as_object().unwrap();
        assert_eq!(selected["selections"][0]["skill_id"], "safe-rust");
        let steps = r["steps"].as_array().unwrap();
        for step in steps {
            let step_selected = step["selected_skills"].as_object().unwrap();
            assert_eq!(step_selected["selections"][0]["skill_id"], "safe-rust");
        }
    }

    #[tokio::test]
    async fn orchestration_persists_bundle_and_records_feedback() {
        let _g = isolate();
        seed_skill_pack();
        let r = OrchestrateRunHandler
            .execute(json!({
                "template": "feature",
                "task": "write a safe rust module with checked arithmetic"
            }))
            .await
            .unwrap();
        let bundle_path = r["selected_skill_bundle_path"].as_str().unwrap();
        assert!(
            std::path::Path::new(bundle_path).exists(),
            "bundle artifact should exist at {}",
            bundle_path
        );
        let bundle_text = std::fs::read_to_string(bundle_path).unwrap();
        assert!(bundle_text.contains("safe-rust"));

        let pack_path = paths::skills_pack_file();
        let store = SkillStore::open(&pack_path).unwrap();
        let feedback = store.get_feedback("safe-rust").unwrap().unwrap();
        assert_eq!(feedback.usage_count, 1);
        assert_eq!(feedback.success_count, 1);
    }

    #[tokio::test]
    async fn code_writing_prompt_exports_real_rust_source() {
        let _g = isolate();
        let r = OrchestrateRunHandler
            .execute(json!({
                "template": "feature",
                "task": "Write a small Rust module that defines a safe add function and unit tests",
                "max_retries": 1
            }))
            .await
            .unwrap();

        let generated = r["generated_sources"].as_array().unwrap();
        let coder_entry = generated
            .iter()
            .find(|entry| entry["archetype"] == "coder")
            .expect("coder should export a source file");
        let source_path = coder_entry["source_path"].as_str().unwrap();
        assert!(
            std::path::Path::new(source_path).exists(),
            "generated source should exist at {}",
            source_path
        );
        let content = std::fs::read_to_string(source_path).unwrap();
        assert!(content.contains("pub fn safe_add"));
        assert!(content.contains("checked_add"));
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
    async fn graph_path_happy_runs_full_pipeline() {
        // max_retries>0 routes through the conditional-edge graph; with no runner
        // every step succeeds, so it follows OnSuccess edges to the terminal node.
        let _g = isolate();
        let r = OrchestrateRunHandler
            .execute(json!({"template": "feature", "task": "x", "max_retries": 2}))
            .await
            .unwrap();
        assert_eq!(r["status"], "completed");
        assert_eq!(r["success"], true);
        assert_eq!(r["step_count"], 4);
        assert_eq!(r["steps"][3]["archetype"], "reviewer");
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
    async fn orchestration_publishes_task_events() {
        let _g = isolate();
        let r = OrchestrateRunHandler
            .execute(json!({"template": "bugfix", "task": "publish task trace"}))
            .await
            .unwrap();
        let orchestration_id = r["orchestration_id"].as_str().unwrap().to_string();

        let events = GovEventsHandler
            .execute(json!({"event_type": "orchestrate.run.completed", "limit": 10}))
            .await
            .unwrap();
        assert!(events["count"].as_u64().unwrap() >= 1);
        assert_eq!(events["events"][0]["task_id"], orchestration_id);
        assert_eq!(
            events["events"][0]["payload"]["orchestration_id"],
            orchestration_id
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_orchestration_emits_repair_events() {
        let _g = isolate();
        let r = OrchestrateRunHandler
            .execute(json!({
                "template": "bugfix",
                "task": "force a repair path",
                "runner": "false"
            }))
            .await
            .unwrap();
        assert_eq!(r["status"], "failed");
        assert_eq!(r["success"], false);

        let events = GovEventsHandler
            .execute(json!({"event_type": "repair.classified", "limit": 10}))
            .await
            .unwrap();
        assert!(events["count"].as_u64().unwrap() >= 1);
        assert_eq!(events["events"][0]["payload"]["failure_class"], "Unknown");
        assert!(events["events"][0]["payload"]["detail"]
            .as_str()
            .unwrap()
            .contains("status failed"));

        let plans = GovEventsHandler
            .execute(json!({"event_type": "repair.plan.generated", "limit": 10}))
            .await
            .unwrap();
        assert!(plans["count"].as_u64().unwrap() >= 1);
        assert_eq!(plans["events"][0]["payload"]["repair_action"], "Abort");
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
