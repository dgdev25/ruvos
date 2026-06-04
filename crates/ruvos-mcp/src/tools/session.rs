//! Session domain tools (3): create, resume, fork.
//!
//! Backed by real signed `.rvf` containers on disk (via `ruvos-session`).
//! Disk is the source of truth, so sessions survive process restarts.

use super::handler::{ExecuteFuture, ToolHandler};
use crate::runtime::{publish_event, RuntimeEvent};
use crate::{paths, Result, RuvosError};
use ruvos_session::{fork_session, read_session, write_session, Session};
use serde_json::{json, Value};

/// Absolute path to a session's `.rvf` file.
///
/// `session_id` must be a plain UUID — this rejects path-traversal payloads
/// (e.g. `../../etc/passwd`) before they can escape the sessions directory.
fn rvf_path_for(session_id: &str) -> Result<String> {
    uuid::Uuid::parse_str(session_id)
        .map_err(|_| RuvosError::InvalidParams("session_id must be a UUID".to_string()))?;
    Ok(paths::sessions_dir()
        .join(format!("{}.rvf", session_id))
        .to_string_lossy()
        .into_owned())
}

// ============================================================================
// session.create
// ============================================================================

pub struct SessionCreateHandler;

impl ToolHandler for SessionCreateHandler {
    fn name(&self) -> &'static str {
        "create"
    }

    fn domain(&self) -> &'static str {
        "session"
    }

    fn validate(&self, params: &Value) -> Result<()> {
        if !params.is_object() {
            return Err(RuvosError::InvalidParams(
                "params must be an object".to_string(),
            ));
        }
        if let Some(name) = params.get("name") {
            if !name.is_string() && !name.is_null() {
                return Err(RuvosError::InvalidParams(
                    "'name' must be a string or null".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let mut session = Session::new();
            session.name = name.clone();
            // session.id is a freshly generated UUID, so this never fails.
            let path = rvf_path_for(&session.id.to_string())?;
            session.rvf_path = path.clone();

            // Optional initial state passed by the caller.
            if let Some(obj) = params.get("state").and_then(|v| v.as_object()) {
                for (k, v) in obj {
                    session.state.insert(k.clone(), v.to_string());
                }
            }

            write_session(&session, &path)
                .await
                .map_err(|e| RuvosError::InternalError(format!("failed to write .rvf: {}", e)))?;

            publish_event(RuntimeEvent {
                kind: "session.created".to_string(),
                payload: json!({
                    "session_id": session.id.to_string(),
                    "name": if name.is_empty() { Value::Null } else { Value::String(name.clone()) },
                    "rvf_path": path.clone(),
                    "state_keys": session.state.keys().cloned().collect::<Vec<_>>(),
                }),
                agent_id: None,
                task_id: None,
            });

            Ok(json!({
                "session_id": session.id.to_string(),
                "name": if name.is_empty() { Value::Null } else { Value::String(name) },
                "rvf_path": path,
                "created_at": session.created_at,
                "status": "created"
            }))
        })
    }
}

// ============================================================================
// session.resume
// ============================================================================

pub struct SessionResumeHandler;

impl ToolHandler for SessionResumeHandler {
    fn name(&self) -> &'static str {
        "resume"
    }

    fn domain(&self) -> &'static str {
        "session"
    }

    fn validate(&self, params: &Value) -> Result<()> {
        if !params.is_object() {
            return Err(RuvosError::InvalidParams(
                "params must be an object".to_string(),
            ));
        }
        if params.get("session_id").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'session_id' field (string)".to_string(),
            ));
        }
        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let session_id = params
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let path = match rvf_path_for(&session_id) {
                Ok(p) => p,
                Err(_) => {
                    return Ok(json!({
                        "session_id": session_id,
                        "status": "not_found",
                        "found": false,
                        "error": "Session not found"
                    }))
                }
            };

            match read_session(&path).await {
                Ok(session) => {
                    let session_id_value = session.id.to_string();
                    let rvf_path_value = session.rvf_path.clone();
                    publish_event(RuntimeEvent {
                        kind: "session.resumed".to_string(),
                        payload: json!({
                            "session_id": session_id_value,
                            "rvf_path": rvf_path_value,
                        }),
                        agent_id: None,
                        task_id: None,
                    });
                    Ok(json!({
                        "session_id": session.id.to_string(),
                        "name": if session.name.is_empty() { Value::Null } else { Value::String(session.name) },
                        "rvf_path": session.rvf_path,
                        "created_at": session.created_at,
                        "updated_at": session.updated_at,
                        "parent_id": session.parent.map(|p| p.to_string()),
                        "state": session.state,
                        "status": "resumed",
                        "found": true
                    }))
                }
                Err(_) => Ok(json!({
                    "session_id": session_id,
                    "status": "not_found",
                    "found": false,
                    "error": "Session not found"
                })),
            }
        })
    }
}

// ============================================================================
// session.fork
// ============================================================================

pub struct SessionForkHandler;

impl ToolHandler for SessionForkHandler {
    fn name(&self) -> &'static str {
        "fork"
    }

    fn domain(&self) -> &'static str {
        "session"
    }

    fn validate(&self, params: &Value) -> Result<()> {
        if !params.is_object() {
            return Err(RuvosError::InvalidParams(
                "params must be an object".to_string(),
            ));
        }
        if params
            .get("source_session_id")
            .and_then(|v| v.as_str())
            .is_none()
        {
            return Err(RuvosError::InvalidParams(
                "missing 'source_session_id' field (string)".to_string(),
            ));
        }
        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let source_session_id = params
                .get("source_session_id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            let source_path = match rvf_path_for(&source_session_id) {
                Ok(p) => p,
                Err(_) => {
                    return Ok(json!({
                        "forked_id": Value::Null,
                        "source_session_id": source_session_id,
                        "status": "source_not_found",
                        "success": false,
                        "error": "Source session not found"
                    }))
                }
            };
            let base_dir = paths::sessions_dir().to_string_lossy().into_owned();

            match fork_session(&source_path, &base_dir).await {
                Ok(child) => {
                    let forked_id = child.id.to_string();
                    let forked_path = child.rvf_path.clone();
                    let source_session_value = source_session_id.clone();
                    publish_event(RuntimeEvent {
                        kind: "session.forked".to_string(),
                        payload: json!({
                            "source_session_id": source_session_value,
                            "forked_id": forked_id,
                            "rvf_path": forked_path,
                        }),
                        agent_id: None,
                        task_id: None,
                    });
                    Ok(json!({
                        "forked_id": child.id.to_string(),
                        "source_session_id": source_session_id,
                        "rvf_path": child.rvf_path,
                        "created_at": child.created_at,
                        "status": "forked",
                        "success": true
                    }))
                }
                Err(_) => Ok(json!({
                    "forked_id": Value::Null,
                    "source_session_id": source_session_id,
                    "status": "source_not_found",
                    "success": false,
                    "error": "Source session not found"
                })),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Give each test a private data dir (thread-local; no cross-test races).
    fn isolate() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(dir.path().to_path_buf());
        dir
    }

    #[tokio::test]
    async fn create_writes_a_real_rvf_file() {
        let _g = isolate();
        let handler = SessionCreateHandler;
        let resp = handler.execute(json!({"name": "test"})).await.unwrap();

        assert_eq!(resp["status"], "created");
        let path = resp["rvf_path"].as_str().unwrap();
        assert!(
            std::path::Path::new(path).exists(),
            "session.create must write a real .rvf file at {}",
            path
        );
    }

    #[tokio::test]
    async fn resume_reads_persisted_session() {
        let _g = isolate();
        let create = SessionCreateHandler
            .execute(json!({"name": "persisted", "state": {"k": "v"}}))
            .await
            .unwrap();
        let id = create["session_id"].as_str().unwrap().to_string();

        let resume = SessionResumeHandler
            .execute(json!({"session_id": id}))
            .await
            .unwrap();
        assert_eq!(resume["status"], "resumed");
        assert_eq!(resume["found"], true);
        assert_eq!(resume["name"], "persisted");
        // state value was stored JSON-encoded
        assert!(resume["state"]["k"].as_str().unwrap().contains('v'));
    }

    #[tokio::test]
    async fn resume_missing_returns_not_found() {
        let _g = isolate();
        let resume = SessionResumeHandler
            .execute(json!({"session_id": "00000000-0000-0000-0000-000000000000"}))
            .await
            .unwrap();
        assert_eq!(resume["status"], "not_found");
        assert_eq!(resume["found"], false);
    }

    #[tokio::test]
    async fn fork_creates_linked_child_on_disk() {
        let _g = isolate();
        let create = SessionCreateHandler
            .execute(json!({"name": "orig", "state": {"shared": "data"}}))
            .await
            .unwrap();
        let source_id = create["session_id"].as_str().unwrap().to_string();

        let fork = SessionForkHandler
            .execute(json!({"source_session_id": source_id.clone()}))
            .await
            .unwrap();
        assert_eq!(fork["status"], "forked");
        assert_eq!(fork["success"], true);

        let forked_id = fork["forked_id"].as_str().unwrap().to_string();
        let resume = SessionResumeHandler
            .execute(json!({"session_id": forked_id}))
            .await
            .unwrap();
        assert_eq!(resume["status"], "resumed");
        assert_eq!(resume["parent_id"].as_str().unwrap(), source_id);
        // COW: child inherited parent's state
        assert!(resume["state"]["shared"].as_str().unwrap().contains("data"));
    }

    #[tokio::test]
    async fn fork_missing_source_fails() {
        let _g = isolate();
        let fork = SessionForkHandler
            .execute(json!({"source_session_id": "00000000-0000-0000-0000-000000000000"}))
            .await
            .unwrap();
        assert_eq!(fork["status"], "source_not_found");
        assert_eq!(fork["success"], false);
    }

    #[test]
    fn validation_rules() {
        assert!(SessionCreateHandler
            .validate(&json!({"name": 123}))
            .is_err());
        assert!(SessionResumeHandler.validate(&json!({})).is_err());
        assert!(SessionForkHandler.validate(&json!({})).is_err());
        assert!(SessionResumeHandler
            .validate(&json!({"session_id": "x"}))
            .is_ok());
    }
}
