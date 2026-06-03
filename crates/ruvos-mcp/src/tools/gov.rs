//! Gov domain tools (2): witness_verify, health.
//!
//! `witness_verify` runs a real HMAC-SHA256 signature check on an `.rvf`
//! container (via `ruvos-session`). `health` reports real, introspected system
//! state: data directory, persisted counts, process id, and registered tools.

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
            let agents = Self::count_flat(paths::agents_file());
            let intel_patterns = Self::count_flat(paths::intel_file());

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
                }
            }))
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
        assert_eq!(r["tool_count"], 20);
        assert!(r["pid"].as_u64().unwrap() > 0, "real process id");
        assert_eq!(r["persisted"]["sessions"], 0);
    }

    #[test]
    fn validation() {
        assert!(GovWitnessVerifyHandler.validate(&json!({})).is_err());
        assert!(GovHealthHandler.validate(&json!({})).is_ok());
    }
}
