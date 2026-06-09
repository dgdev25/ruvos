//! Gov domain tools (10): witness_verify, health, events, replay, report, swarm_policy, swarm_history, swarm_recommendation, swarm_plan, swarm_status.
//!
//! `witness_verify` runs a real HMAC-SHA256 signature check on an `.rvf`
//! container (via `ruvos-session`). `health` reports real, introspected system
//! state: data directory, persisted counts, process id, and registered tools.
//! `events` queries the signed audit/event log persisted by `ruvos-store`.

use super::handler::{ExecuteFuture, ToolHandler};
use crate::{
    constants::{DEFAULT_EVENT_LIMIT, GOV_REPLAY_LIMIT, GOV_SWARM_HISTORY_LIMIT},
    paths, swarm, Result, RuvosError,
};
use serde_json::{json, Value};

fn read_json<T: serde::de::DeserializeOwned + Default>(path: std::path::PathBuf) -> T {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<T>(&text).ok())
        .unwrap_or_default()
}

// ============================================================================
// gov.witness_verify
// ============================================================================

pub struct GovWitnessVerifyHandler;

impl ToolHandler for GovWitnessVerifyHandler {
    fn name(&self) -> &'static str {
        "witness_verify"
    }
    fn domain(&self) -> &'static str {
        "gov"
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["rvf_path"],
            "properties": {
                "rvf_path": { "type": "string", "description": "Absolute path to a .rvf session file (not a directory)" }
            },
            "additionalProperties": false
        })
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("rvf_path").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'rvf_path' field (string)".to_string(),
            ));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let rvf_path = params["rvf_path"].as_str().unwrap_or_default().to_string();

            // Confine verification to the rUvOS data root: reject any path that
            // (once resolved) escapes it, preventing reads of arbitrary files.
            if let Ok(canonical) = std::fs::canonicalize(&rvf_path) {
                let base = std::fs::canonicalize(paths::data_root())
                    .unwrap_or_else(|_| paths::data_root());
                if !canonical.starts_with(&base) {
                    return Ok(json!({
                        "rvf_path": rvf_path,
                        "verified": false,
                        "exists": true,
                        "error": "path outside the rUvOS data directory"
                    }));
                }
            }

            match ruvos_session::verify_signature(&rvf_path).await {
                Ok(verified) => Ok(json!({
                    "rvf_path": rvf_path,
                    "verified": verified,
                    "exists": true
                })),
                Err(e) => Ok(json!({
                    "rvf_path": rvf_path,
                    "verified": false,
                    "exists": false,
                    "error": e.to_string()
                })),
            }
        })
    }
}

// ============================================================================
// gov.health
// ============================================================================

pub struct GovHealthHandler;

impl GovHealthHandler {
    /// Count top-level entries in a flat `{id: record}` object, or array length.
    fn count_flat(path: std::path::PathBuf) -> u64 {
        match std::fs::read(&path) {
            Ok(b) => match serde_json::from_slice::<Value>(&b) {
                Ok(Value::Object(map)) => map.len() as u64,
                Ok(Value::Array(a)) => a.len() as u64,
                _ => 0,
            },
            Err(_) => 0,
        }
    }

    /// Count leaf entries in a nested `{namespace: {key: entry}}` object.
    fn count_nested(path: std::path::PathBuf) -> u64 {
        match std::fs::read(&path) {
            Ok(b) => match serde_json::from_slice::<Value>(&b) {
                Ok(Value::Object(map)) => map
                    .values()
                    .map(|v| v.as_object().map(|o| o.len() as u64).unwrap_or(0))
                    .sum(),
                _ => 0,
            },
            Err(_) => 0,
        }
    }
}

impl ToolHandler for GovHealthHandler {
    fn name(&self) -> &'static str {
        "health"
    }
    fn domain(&self) -> &'static str {
        "gov"
    }
    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }
    fn execute(&self, _params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let root = paths::data_root();
            let root_exists = root.exists();

            // Real counts from disk.
            let sessions = std::fs::read_dir(paths::sessions_dir())
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .filter(|e| e.path().extension().map(|x| x == "rvf").unwrap_or(false))
                        .count() as u64
                })
                .unwrap_or(0);
            let memory_entries = Self::count_nested(paths::memory_file());
            // Agents now live in the redb-backed store, not a flat JSON file.
            // Best-effort: 0 if the store is held by another instance.
            let agents = crate::store::try_store()
                .and_then(|s| s.list_agents().ok())
                .map(|a| a.len() as u64)
                .unwrap_or(0);
            let intel_patterns = Self::count_flat(paths::intel_file());

            // Safety subsystem introspection via the shared SafetyEngine.
            let (safety_score, active_constraints, recent_violations) = {
                let engine = crate::safety::engine();
                let guard = engine.lock().unwrap_or_else(|p| p.into_inner());
                let one_hour_ago = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
                (
                    guard.safety_score(),
                    guard.constraints().len() as u64,
                    guard.violations_since(&one_hour_ago).len() as u64,
                )
            };

            Ok(json!({
                "status": "ok",
                "version": env!("CARGO_PKG_VERSION"),
                "pid": std::process::id(),
                "data_root": root.to_string_lossy(),
                "data_root_exists": root_exists,
                "tool_count": crate::tools::public_tool_count(),
                "persisted": {
                    "sessions": sessions,
                    "memory_entries": memory_entries,
                    "agents": agents,
                    "intel_patterns": intel_patterns
                },
                "subsystems": {
                    "mcp": "ok",
                    "session": "ok",
                    "memory": "ok",
                    "plugin": "ok",
                    "hooks": "ok"
                },
                "safety": {
                    "score": safety_score,
                    "active_constraints": active_constraints,
                    "recent_violations": recent_violations
                }
            }))
        })
    }
}

// ============================================================================
// gov.events
// ============================================================================

/// Query the signed audit/event log persisted by `ruvos-store`.
///
/// Params (all optional): `since` (unix secs, default 0), `agent_id`,
/// `event_type`, `limit` (default 50). When `agent_id` or `event_type` is
/// given, the corresponding indexed query is used; otherwise a time-range scan
/// from `since` is returned.
pub struct GovEventsHandler;

impl ToolHandler for GovEventsHandler {
    fn name(&self) -> &'static str {
        "events"
    }
    fn domain(&self) -> &'static str {
        "gov"
    }
    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let since = params.get("since").and_then(|v| v.as_i64()).unwrap_or(0);
            let limit = params
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(DEFAULT_EVENT_LIMIT as u64) as usize;
            let agent_id = params.get("agent_id").and_then(|v| v.as_str());
            let event_type = params.get("event_type").and_then(|v| v.as_str());

            // Best-effort: if the store is held by another instance, report an
            // empty (but successful) result rather than failing the call.
            let Some(s) = crate::store::try_store() else {
                return Ok(json!({ "count": 0, "events": [], "store_busy": true }));
            };
            let events = if let Some(id) = agent_id {
                s.events_by_agent(id, limit)
            } else if let Some(et) = event_type {
                s.events_by_type(et, limit)
            } else {
                s.events_since(since).map(|mut v| {
                    v.truncate(limit);
                    v
                })
            }
            .map_err(|e| RuvosError::InternalError(format!("events query: {}", e)))?;

            let out: Vec<Value> = events
                .iter()
                .map(|e| {
                    json!({
                        "id": e.id,
                        "event_type": e.event_type,
                        "agent_id": e.agent_id,
                        "task_id": e.task_id,
                        "payload": e.payload,
                        "timestamp": e.timestamp
                    })
                })
                .collect();

            Ok(json!({ "count": out.len(), "events": out }))
        })
    }
}

// ============================================================================
// gov.replay
// ============================================================================

pub struct GovReplayHandler;

impl ToolHandler for GovReplayHandler {
    fn name(&self) -> &'static str {
        "replay"
    }
    fn domain(&self) -> &'static str {
        "gov"
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("session_id").and_then(|v| v.as_str()).is_none()
            && params.get("task_id").and_then(|v| v.as_str()).is_none()
        {
            return Err(RuvosError::InvalidParams(
                "missing 'session_id' or 'task_id' field (string)".to_string(),
            ));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let session_id = params.get("session_id").and_then(|v| v.as_str());
            let task_id = params.get("task_id").and_then(|v| v.as_str());
            let limit = params
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(GOV_REPLAY_LIMIT as u64) as usize;

            let Some(store) = crate::store::try_store() else {
                return Ok(json!({ "count": 0, "events": [], "store_busy": true }));
            };
            let events = store
                .events_since(0)
                .map_err(|e| RuvosError::InternalError(format!("replay events: {e}")))?;

            let mut trace: Vec<Value> = events
                .into_iter()
                .filter(|event| {
                    let payload_session = event.payload.get("session_id").and_then(|v| v.as_str());
                    let payload_task = event.payload.get("task_id").and_then(|v| v.as_str());
                    session_id
                        .map(|wanted| payload_session == Some(wanted))
                        .unwrap_or(true)
                        && task_id
                            .map(|wanted| payload_task == Some(wanted))
                            .unwrap_or(true)
                })
                .map(|event| {
                    json!({
                        "id": event.id,
                        "event_type": event.event_type,
                        "agent_id": event.agent_id,
                        "task_id": event.task_id,
                        "payload": event.payload,
                        "timestamp": event.timestamp
                    })
                })
                .collect();

            trace.sort_by(|a, b| a["timestamp"].as_str().cmp(&b["timestamp"].as_str()));
            trace.truncate(limit);

            let replay = if let Some(session_id) = session_id {
                let path = paths::sessions_dir().join(format!("{}.rvf", session_id));
                let session = if path.exists() {
                    match ruvos_session::read_session(path.to_string_lossy().as_ref()).await {
                        Ok(session) => Some(json!({
                            "session_id": session.id.to_string(),
                            "name": if session.name.is_empty() { Value::Null } else { Value::String(session.name) },
                            "rvf_path": session.rvf_path,
                            "created_at": session.created_at,
                            "updated_at": session.updated_at,
                            "parent_id": session.parent.map(|p| p.to_string()),
                            "state": session.state,
                        })),
                        Err(_) => None,
                    }
                } else {
                    None
                };
                json!({
                    "session": session,
                    "session_id": session_id,
                })
            } else {
                json!({
                    "task_id": task_id,
                })
            };

            Ok(json!({
                "count": trace.len(),
                "replay": replay,
                "events": trace
            }))
        })
    }
}

// ============================================================================
// gov.report
// ============================================================================

pub struct GovReportHandler;

impl ToolHandler for GovReportHandler {
    fn name(&self) -> &'static str {
        "report"
    }
    fn domain(&self) -> &'static str {
        "gov"
    }
    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let since = params.get("since").and_then(|v| v.as_i64()).unwrap_or(0);
            let Some(store) = crate::store::try_store() else {
                return Ok(json!({ "store_busy": true, "report": {} }));
            };
            let events = store
                .events_since(since)
                .map_err(|e| RuvosError::InternalError(format!("report events: {e}")))?;

            let total = events.len() as u64;
            let agent_spawns = events
                .iter()
                .filter(|event| event.event_type == "agent.spawn.completed")
                .count() as u64;
            let agent_failures = events
                .iter()
                .filter(|event| event.event_type == "agent.spawn.failed")
                .count() as u64;
            let orchestrate_runs = events
                .iter()
                .filter(|event| event.event_type == "orchestrate.run.completed")
                .count() as u64
                + events
                    .iter()
                    .filter(|event| event.event_type == "orchestrate.run.failed")
                    .count() as u64;
            let orchestrate_failures = events
                .iter()
                .filter(|event| event.event_type == "orchestrate.run.failed")
                .count() as u64;
            let repair_events = events
                .iter()
                .filter(|event| event.event_type.starts_with("repair."))
                .count() as u64;
            let relay_contracts = events
                .iter()
                .filter(|event| event.event_type == "relay.contract.stored")
                .count() as u64;
            let replayable_sessions = std::fs::read_dir(paths::sessions_dir())
                .map(|rd| {
                    rd.filter_map(|entry| entry.ok())
                        .filter(|entry| {
                            entry
                                .path()
                                .extension()
                                .map(|ext| ext == "rvf")
                                .unwrap_or(false)
                        })
                        .count() as u64
                })
                .unwrap_or(0);
            let swarm_snapshot = {
                let current = swarm::current();
                let policy = read_json::<Value>(paths::swarm_policy_file());
                let history = read_json::<Value>(paths::swarm_history_file());
                let learning = read_json::<Value>(paths::swarm_learning_file());
                json!({
                    "current": current,
                    "policy": policy,
                    "history": history,
                    "learning": learning,
                })
            };
            let success_rate = if orchestrate_runs == 0 {
                1.0
            } else {
                (orchestrate_runs - orchestrate_failures) as f64 / orchestrate_runs as f64
            };

            Ok(json!({
                "since": since,
                "report": {
                    "event_count": total,
                    "success_rate": success_rate,
                    "agent_spawns": agent_spawns,
                    "agent_failures": agent_failures,
                    "orchestrate_runs": orchestrate_runs,
                    "orchestrate_failures": orchestrate_failures,
                    "repair_events": repair_events,
                    "relay_contracts": relay_contracts,
                    "replayable_sessions": replayable_sessions,
                    "swarm": swarm_snapshot,
                    "tool_count": crate::tools::public_tool_count(),
                }
            }))
        })
    }
}

// ============================================================================
// gov.swarm_recommendation
// ============================================================================

pub struct GovSwarmRecommendationHandler;

impl ToolHandler for GovSwarmRecommendationHandler {
    fn name(&self) -> &'static str {
        "swarm_recommendation"
    }
    fn domain(&self) -> &'static str {
        "gov"
    }
    fn validate(&self, params: &Value) -> Result<()> {
        let has_objective = params.get("objective").and_then(|v| v.as_str()).is_some();
        let has_task = params.get("task").and_then(|v| v.as_str()).is_some();
        let has_goal = params.get("goal").and_then(|v| v.as_str()).is_some();
        if has_objective || has_task || has_goal {
            Ok(())
        } else {
            Err(RuvosError::InvalidParams(
                "missing 'objective', 'task', or 'goal' field (string)".to_string(),
            ))
        }
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let member_count = params
                .get("members")
                .and_then(|v| v.as_array())
                .map(|members| members.len())
                .unwrap_or_else(|| {
                    params
                        .get("member_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize
                });
            let max_agents = params
                .get("max_agents")
                .and_then(|v| v.as_u64())
                .unwrap_or(6) as u32;
            let recommendation =
                super::swarm::recommend_topology(&params, member_count, max_agents);

            let assignment_hint = match recommendation.topology.as_str() {
                "mesh" => {
                    "Use peer fan-out, direct member-to-member messaging, and broad coordination."
                }
                "hybrid" => {
                    "Use a coordinator-led plan with parallel coder/tester roles and rebalance if needed."
                }
                "adaptive" => {
                    "Keep roles fluid and revisit the topology after checkpoints or recovery events."
                }
                _ => {
                    "Use a coordinator-led handoff chain with a small number of clearly owned tasks."
                }
            };
            let provided_roles: Vec<String> = params
                .get("members")
                .and_then(|v| v.as_array())
                .map(|members| {
                    members
                        .iter()
                        .filter_map(|member| member.get("role").and_then(|v| v.as_str()))
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();

            Ok(json!({
                "recommended_topology": recommendation.topology,
                "source": recommendation.source,
                "reason": recommendation.reason,
                "assignment_hint": assignment_hint,
                "member_count": member_count,
                "max_agents": max_agents,
                "provided_roles": provided_roles,
            }))
        })
    }
}

// ============================================================================
// gov.swarm_plan
// ============================================================================

pub struct GovSwarmPlanHandler;

impl ToolHandler for GovSwarmPlanHandler {
    fn name(&self) -> &'static str {
        "swarm_plan"
    }
    fn domain(&self) -> &'static str {
        "gov"
    }
    fn validate(&self, params: &Value) -> Result<()> {
        let has_objective = params.get("objective").and_then(|v| v.as_str()).is_some();
        let has_task = params.get("task").and_then(|v| v.as_str()).is_some();
        let has_goal = params.get("goal").and_then(|v| v.as_str()).is_some();
        if has_objective || has_task || has_goal {
            Ok(())
        } else {
            Err(RuvosError::InvalidParams(
                "missing 'objective', 'task', or 'goal' field (string)".to_string(),
            ))
        }
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let member_count = params
                .get("members")
                .and_then(|v| v.as_array())
                .map(|members| members.len())
                .unwrap_or_else(|| {
                    params
                        .get("member_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as usize
                });
            let max_agents = params
                .get("max_agents")
                .and_then(|v| v.as_u64())
                .unwrap_or(6) as u32;
            let plan = swarm::recommend_plan(&params, member_count, max_agents);
            Ok(plan)
        })
    }
}

// ============================================================================
// gov.swarm_status
// ============================================================================

pub struct GovSwarmStatusHandler;

impl ToolHandler for GovSwarmStatusHandler {
    fn name(&self) -> &'static str {
        "swarm_status"
    }
    fn domain(&self) -> &'static str {
        "gov"
    }
    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }
    fn execute(&self, _params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let Some(state) = swarm::current() else {
                return Ok(json!({
                    "exists": false,
                    "status": "inactive",
                }));
            };
            let plan = swarm::recommend_plan(
                &json!({
                    "objective": state.objective,
                    "topology": state.topology,
                    "members": state.members.iter().map(|member| json!({
                        "agent_id": member.agent_id,
                        "role": member.role,
                    })).collect::<Vec<_>>(),
                    "max_agents": state.max_agents,
                }),
                state.members.len(),
                state.max_agents,
            );
            Ok(json!({
                "exists": true,
                "state": state,
                "plan": plan,
            }))
        })
    }
}

// ============================================================================
// gov.swarm_policy
// ============================================================================

pub struct GovSwarmPolicyHandler;

impl ToolHandler for GovSwarmPolicyHandler {
    fn name(&self) -> &'static str {
        "swarm_policy"
    }
    fn domain(&self) -> &'static str {
        "gov"
    }
    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let signature = params.get("signature").and_then(|v| v.as_str());
            let policy: swarm::SwarmPolicy = read_json(paths::swarm_policy_file());
            let entries: Vec<Value> = policy
                .entries
                .values()
                .filter(|entry| {
                    signature
                        .map(|wanted| entry.signature == wanted)
                        .unwrap_or(true)
                })
                .map(|entry| {
                    json!({
                        "signature": entry.signature,
                        "preferred_topology": entry.preferred_topology,
                        "success_count": entry.success_count,
                        "failure_count": entry.failure_count,
                        "last_outcome": entry.last_outcome,
                        "updated_at": entry.updated_at,
                    })
                })
                .collect();
            Ok(json!({
                "version": policy.version,
                "count": entries.len(),
                "entries": entries,
            }))
        })
    }
}

// ============================================================================
// gov.swarm_history
// ============================================================================

pub struct GovSwarmHistoryHandler;

impl ToolHandler for GovSwarmHistoryHandler {
    fn name(&self) -> &'static str {
        "swarm_history"
    }
    fn domain(&self) -> &'static str {
        "gov"
    }
    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let limit = params
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(GOV_SWARM_HISTORY_LIMIT as u64) as usize;
            let signature = params.get("signature").and_then(|v| v.as_str());
            let status = params.get("status").and_then(|v| v.as_str());
            let mut history: swarm::SwarmRunHistory = read_json(paths::swarm_history_file());
            history
                .records
                .sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
            let records: Vec<Value> = history
                .records
                .into_iter()
                .filter(|record| {
                    signature
                        .map(|wanted| record.signature == wanted)
                        .unwrap_or(true)
                        && status.map(|wanted| record.status == wanted).unwrap_or(true)
                })
                .take(limit)
                .map(|record| {
                    json!({
                        "signature": record.signature,
                        "objective": record.objective,
                        "topology": record.topology,
                        "status": record.status,
                        "detail": record.detail,
                        "member_count": record.member_count,
                        "max_agents": record.max_agents,
                        "updated_at": record.updated_at,
                    })
                })
                .collect();
            Ok(json!({
                "version": history.version,
                "count": records.len(),
                "records": records,
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::session::SessionCreateHandler;
    use ruvos_session::{write_session, Session};

    fn isolate() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(dir.path().to_path_buf());
        dir
    }

    #[tokio::test]
    async fn witness_verify_accepts_valid_container_and_rejects_tampered() {
        let dir = isolate();
        let path = dir.path().join("good.rvf");
        let path_str = path.to_str().unwrap();

        let mut s = Session::new();
        s.name = "signed".into();
        write_session(&s, path_str).await.unwrap();

        let ok = GovWitnessVerifyHandler
            .execute(json!({"rvf_path": path_str}))
            .await
            .unwrap();
        assert_eq!(ok["verified"], true, "valid container must verify");

        // Tamper the file on disk.
        let raw = std::fs::read_to_string(path_str).unwrap();
        std::fs::write(path_str, raw.replace("signed", "forged")).unwrap();
        let bad = GovWitnessVerifyHandler
            .execute(json!({"rvf_path": path_str}))
            .await
            .unwrap();
        assert_eq!(bad["verified"], false, "tampered container must fail");
    }

    #[tokio::test]
    async fn witness_verify_missing_file_reports_not_exists() {
        let _g = isolate();
        let r = GovWitnessVerifyHandler
            .execute(json!({"rvf_path": "/nonexistent/path.rvf"}))
            .await
            .unwrap();
        assert_eq!(r["verified"], false);
        assert_eq!(r["exists"], false);
    }

    #[tokio::test]
    async fn health_reports_real_state() {
        let _g = isolate();
        let r = GovHealthHandler.execute(json!({})).await.unwrap();
        assert_eq!(r["status"], "ok");
        assert_eq!(r["tool_count"], crate::tools::public_tool_count());
        assert!(r["pid"].as_u64().unwrap() > 0, "real process id");
        assert_eq!(r["persisted"]["sessions"], 0);
    }

    #[tokio::test]
    async fn health_includes_safety_score() {
        let _g = isolate();
        let r = GovHealthHandler.execute(json!({})).await.unwrap();

        // Safety subsystem must be reported with a score and constraint count.
        let safety = &r["safety"];
        let score = safety["score"].as_f64().expect("safety.score is a number");
        assert!(
            (0.0..=1.0).contains(&score),
            "safety score must be in [0,1], got {score}"
        );
        // The engine ships with 5 default constraints.
        assert_eq!(safety["active_constraints"], 5);
        assert_eq!(safety["recent_violations"], 0);
    }

    #[tokio::test]
    async fn events_returns_spawn_audit_event() {
        let _g = isolate();
        // Spawning an agent appends an `agent.spawned` event to the store.
        super::super::agent::AgentSpawnHandler
            .execute(json!({"archetype": "coder", "prompt": "x", "model": "m"}))
            .await
            .unwrap();

        let r = GovEventsHandler.execute(json!({})).await.unwrap();
        let events = r["events"].as_array().unwrap();
        assert!(
            events.iter().any(|e| e["event_type"] == "agent.spawned"),
            "events must include the spawn audit record: {:?}",
            events
        );

        // Filtering by type narrows the result set.
        let by_type = GovEventsHandler
            .execute(json!({"event_type": "agent.spawned"}))
            .await
            .unwrap();
        assert!(by_type["count"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn replay_returns_session_trace() {
        let _g = isolate();
        let created = SessionCreateHandler
            .execute(json!({"name": "traceable"}))
            .await
            .unwrap();
        let session_id = created["session_id"].as_str().unwrap().to_string();
        super::super::agent::AgentSpawnHandler
            .execute(json!({"archetype": "coder", "prompt": "trace", "model": "m"}))
            .await
            .unwrap();

        let replay = GovReplayHandler
            .execute(json!({"session_id": session_id}))
            .await
            .unwrap();
        assert!(replay["count"].as_u64().unwrap() >= 1);
        assert_eq!(replay["replay"]["session_id"], created["session_id"]);
    }

    #[tokio::test]
    async fn report_summarizes_system_state() {
        let _g = isolate();
        super::super::agent::AgentSpawnHandler
            .execute(json!({"archetype": "coder", "prompt": "report", "model": "m"}))
            .await
            .unwrap();
        super::super::swarm::SwarmCreateHandler
            .execute(json!({
                "objective": "broadcast updates across peer workers",
                "members": [{"agent_id": "worker-1", "role": "coder"}]
            }))
            .await
            .unwrap();
        super::super::swarm::SwarmCompleteHandler
            .execute(json!({
                "summary": "done",
                "completed_by": "worker-1"
            }))
            .await
            .unwrap();

        let report = GovReportHandler.execute(json!({})).await.unwrap();
        assert!(report["report"]["event_count"].as_u64().unwrap() >= 1);
        assert_eq!(
            report["report"]["tool_count"],
            crate::tools::public_tool_count()
        );
        assert!(report["report"]["success_rate"].as_f64().unwrap() >= 0.0);
        assert!(report["report"]["swarm"]["current"].is_object());
        assert!(report["report"]["swarm"]["policy"].is_object());
        assert!(report["report"]["swarm"]["history"].is_object());
    }

    #[tokio::test]
    async fn swarm_policy_and_history_are_queryable() {
        let _g = isolate();

        super::super::swarm::SwarmCreateHandler
            .execute(json!({
                "objective": "broadcast updates across peer workers",
                "members": [{"agent_id": "worker-1", "role": "coder"}]
            }))
            .await
            .unwrap();
        super::super::swarm::SwarmCompleteHandler
            .execute(json!({
                "summary": "mesh run finished",
                "completed_by": "worker-1"
            }))
            .await
            .unwrap();

        let policy = GovSwarmPolicyHandler.execute(json!({})).await.unwrap();
        assert!(policy["count"].as_u64().unwrap() >= 1);
        assert!(policy["entries"].is_array());

        let plan = GovSwarmPlanHandler
            .execute(json!({
                "objective": "broadcast updates across peer workers",
                "members": [{"agent_id": "worker-1", "role": "coder"}]
            }))
            .await
            .unwrap();
        assert_eq!(plan["recommended_topology"], "mesh");
        assert!(plan["phases"].is_array());

        let status = GovSwarmStatusHandler.execute(json!({})).await.unwrap();
        assert!(status["exists"].as_bool().unwrap());
        assert!(status["plan"].is_object());

        let history = GovSwarmHistoryHandler
            .execute(json!({"limit": 5}))
            .await
            .unwrap();
        assert!(history["count"].as_u64().unwrap() >= 1);
        assert!(history["records"].is_array());
        assert_eq!(
            history["records"][0]["topology"],
            policy["entries"][0]["preferred_topology"]
        );
    }

    #[test]
    fn validation() {
        assert!(GovWitnessVerifyHandler.validate(&json!({})).is_err());
        assert!(GovHealthHandler.validate(&json!({})).is_ok());
        assert!(GovEventsHandler.validate(&json!({})).is_ok());
        assert!(GovReplayHandler.validate(&json!({})).is_err());
        assert!(GovReportHandler.validate(&json!({})).is_ok());
        assert!(GovSwarmRecommendationHandler.validate(&json!({})).is_err());
        assert!(GovSwarmPlanHandler.validate(&json!({})).is_err());
        assert!(GovSwarmStatusHandler.validate(&json!({})).is_ok());
        assert!(GovSwarmPolicyHandler.validate(&json!({})).is_ok());
        assert!(GovSwarmHistoryHandler.validate(&json!({})).is_ok());
    }
}
