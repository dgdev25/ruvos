//! gov_sprint_summary tool (ADR-024).
//!
//! `ruvos_gov_sprint_summary` aggregates a sprint's activity from persisted
//! swarm state + the ruvos event log. Pass the `sprint_id` that was supplied at
//! `ruvos_swarm_create` time; the tool finds the matching swarm and summarises:
//!
//! - duration_ms      — wall time from swarm created_at to updated_at
//! - agents_used      — member archetypes/roles that were active
//! - tasks            — {completed, failed, total} from the task graph
//! - test_delta       — baseline vs current test count (baseline from swarm metadata)
//! - events_count     — raw event count in the sprint window
//! - commits          — recent git commits (git log --oneline -10, best-effort)

use super::handler::{ExecuteFuture, ToolHandler};
use crate::{store::try_store, swarm, Result, RuvosError};
use serde_json::{json, Value};

pub struct GovSprintSummaryHandler;

impl ToolHandler for GovSprintSummaryHandler {
    fn name(&self) -> &'static str {
        "ruvos_gov_sprint_summary"
    }
    fn domain(&self) -> &'static str {
        "gov"
    }
    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["sprint_id"],
            "properties": {
                "sprint_id": {
                    "type": "string",
                    "description": "The sprint tag passed to ruvos_swarm_create"
                },
                "final_tests": {
                    "type": "integer",
                    "description": "Current passing test count (for test_delta calculation)"
                }
            }
        })
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("sprint_id").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams("sprint_id required".into()));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let sprint_id = params["sprint_id"].as_str().unwrap_or_default().to_string();
            let final_tests = params["final_tests"].as_u64().map(|n| n as u32);

            // Find the swarm tagged with this sprint_id.
            let state = swarm::current();
            let sprint_swarm = state.and_then(|s| {
                if s.sprint_id.as_deref() == Some(&sprint_id) {
                    Some(s)
                } else {
                    None
                }
            });

            let (duration_ms, agents_used, tasks, baseline_tests, swarm_id, created_ts) =
                if let Some(ref sw) = sprint_swarm {
                    let dur = parse_duration_ms(&sw.created_at, &sw.updated_at);
                    let agents: Vec<String> = sw
                        .members
                        .iter()
                        .filter(|m| m.state == "active" || m.state == "assigned")
                        .map(|m| m.role.clone())
                        .collect::<std::collections::HashSet<_>>()
                        .into_iter()
                        .collect();

                    let tg = &sw.task_graph;
                    let completed = tg
                        .nodes
                        .values()
                        .filter(|n| matches!(n.state, crate::runtime::TaskState::Completed))
                        .count() as u32;
                    let failed = tg
                        .nodes
                        .values()
                        .filter(|n| matches!(n.state, crate::runtime::TaskState::Failed))
                        .count() as u32;
                    let total = tg.nodes.len() as u32;

                    (
                        dur,
                        agents,
                        json!({"completed": completed, "failed": failed, "total": total}),
                        sw.baseline_tests,
                        sw.id.clone(),
                        sw.created_at.clone(),
                    )
                } else {
                    (
                        0u64,
                        vec![],
                        json!({"completed": 0, "failed": 0, "total": 0}),
                        None,
                        "unknown".to_string(),
                        chrono::Utc::now().to_rfc3339(),
                    )
                };

            // Count events in the sprint window (best-effort).
            let events_count = count_sprint_events(&created_ts);

            // Test delta.
            let test_delta = match (baseline_tests, final_tests) {
                (Some(b), Some(f)) => {
                    json!({"before": b, "after": f, "added": (f as i64 - b as i64)})
                }
                (Some(b), None) => json!({"before": b, "after": null, "added": null}),
                _ => json!(null),
            };

            // Recent git commits (best-effort; empty list if git unavailable).
            let commits = git_log_oneline(10).await;

            let status = if sprint_swarm.is_some() {
                "ok"
            } else {
                "sprint_not_found"
            };

            Ok(json!({
                "status": status,
                "sprint_id": sprint_id,
                "swarm_id": swarm_id,
                "duration_ms": duration_ms,
                "agents_used": agents_used,
                "tasks": tasks,
                "test_delta": test_delta,
                "events_count": events_count,
                "commits": commits,
            }))
        })
    }
}

fn parse_duration_ms(created: &str, updated: &str) -> u64 {
    let parse = |s: &str| {
        chrono::DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.timestamp_millis())
            .ok()
    };
    match (parse(created), parse(updated)) {
        (Some(c), Some(u)) if u >= c => (u - c) as u64,
        _ => 0,
    }
}

fn count_sprint_events(created_at: &str) -> u64 {
    let ts = chrono::DateTime::parse_from_rfc3339(created_at)
        .map(|dt| dt.timestamp())
        .unwrap_or(0);
    try_store()
        .and_then(|store| store.events_since(ts).ok())
        .map(|evs| evs.len() as u64)
        .unwrap_or(0)
}

async fn git_log_oneline(n: usize) -> Vec<String> {
    let out = tokio::process::Command::new("git")
        .args(["log", "--oneline", &format!("-{n}")])
        .output()
        .await;
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(str::to_string)
            .collect(),
        _ => vec![],
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn isolate() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(dir.path().to_path_buf());
        dir
    }

    #[test]
    fn validate_rejects_missing_sprint_id() {
        assert!(GovSprintSummaryHandler.validate(&json!({})).is_err());
    }

    #[test]
    fn validate_accepts_sprint_id() {
        assert!(GovSprintSummaryHandler
            .validate(&json!({"sprint_id": "sprint-1"}))
            .is_ok());
    }

    #[tokio::test]
    async fn summary_returns_sprint_not_found_for_unknown_sprint() {
        let _g = isolate();
        let r = GovSprintSummaryHandler
            .execute(json!({"sprint_id": "nonexistent-sprint"}))
            .await
            .unwrap();
        assert_eq!(r["status"], "sprint_not_found");
        assert_eq!(r["sprint_id"], "nonexistent-sprint");
    }

    #[tokio::test]
    async fn summary_returns_ok_for_tagged_swarm() {
        let _g = isolate();

        // Create a swarm with sprint_id via the handler.
        use crate::tools::handler::ToolHandler as TH;
        use crate::tools::swarm::SwarmCreateHandler;
        SwarmCreateHandler
            .execute(json!({
                "objective": "test sprint",
                "sprint_id": "sprint-42",
                "baseline_tests": 100
            }))
            .await
            .unwrap();

        let r = GovSprintSummaryHandler
            .execute(json!({"sprint_id": "sprint-42", "final_tests": 115}))
            .await
            .unwrap();

        assert_eq!(r["status"], "ok");
        assert_eq!(r["sprint_id"], "sprint-42");
        assert_eq!(r["test_delta"]["before"], 100);
        assert_eq!(r["test_delta"]["after"], 115);
        assert_eq!(r["test_delta"]["added"], 15);
    }

    #[test]
    fn parse_duration_ms_works() {
        let created = "2026-01-01T00:00:00Z";
        let updated = "2026-01-01T00:01:00Z";
        assert_eq!(parse_duration_ms(created, updated), 60_000);
    }

    #[test]
    fn parse_duration_ms_returns_zero_on_bad_input() {
        assert_eq!(parse_duration_ms("bad", "bad"), 0);
    }
}
