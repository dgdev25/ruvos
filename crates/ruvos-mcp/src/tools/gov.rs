//! Gov domain tools (3): witness_verify, health, events.
//!
//! `witness_verify` runs a real HMAC-SHA256 signature check on an `.rvf`
//! container (via `ruvos-session`). `health` reports real, introspected system
//! state: data directory, persisted counts, process id, and registered tools.
//! `events` queries the signed audit/event log persisted by `ruvos-store`.

use super::handler::{ExecuteFuture, ToolHandler};
use crate::{paths, Result, RuvosError};
use serde_json::{json, Value};

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
                "tool_count": crate::tools::tool_registry().len(),
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
            let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
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

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(r["tool_count"], 24);
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

    #[test]
    fn validation() {
        assert!(GovWitnessVerifyHandler.validate(&json!({})).is_err());
        assert!(GovHealthHandler.validate(&json!({})).is_ok());
        assert!(GovEventsHandler.validate(&json!({})).is_ok());
    }
}
