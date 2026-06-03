//! Session domain tools (3): create, resume, fork
//!
//! Phase 5v1 implementation with in-memory storage.
//! Real .rvf container integration deferred to Phase 5 refinement.

use super::handler::{ExecuteFuture, ToolHandler};
use crate::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: SessionId,
    pub name: Option<String>,
    pub rvf_path: String,
    pub created_at: String,
    pub parent_id: Option<String>, // For fork tracking
}

// In-memory storage: session_id -> metadata
type SessionStore = Arc<RwLock<HashMap<String, SessionMetadata>>>;

// Global session store instance (Phase 5v1 only; Phase 5+ will use RVF containers)
lazy_static::lazy_static! {
    static ref SESSION_STORE: SessionStore = Arc::new(RwLock::new(HashMap::new()));
}

// ============================================================================
// session.create handler
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
            return Err(crate::rUvOSError::InvalidParams(
                "params must be an object".to_string(),
            ));
        }

        // Optional 'name' field validation
        if let Some(name) = params.get("name") {
            if !name.is_string() && !name.is_null() {
                return Err(crate::rUvOSError::InvalidParams(
                    "'name' must be a string or null".to_string(),
                ));
            }
        }

        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let session_id = Uuid::new_v4();
            let session_id_str = session_id.to_string();
            let now = Utc::now().to_rfc3339();

            // Extract optional name
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let rvf_path = format!(".rvf/{}", session_id_str);

            let metadata = SessionMetadata {
                id: SessionId(session_id),
                name: name.clone(),
                rvf_path: rvf_path.clone(),
                created_at: now.clone(),
                parent_id: None,
            };

            // Store in memory
            let mut store = SESSION_STORE.write().unwrap();
            store.insert(session_id_str.clone(), metadata);

            Ok(json!({
                "session_id": session_id_str,
                "name": name,
                "rvf_path": rvf_path,
                "created_at": now,
                "status": "created"
            }))
        })
    }
}

// ============================================================================
// session.resume handler
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
            return Err(crate::rUvOSError::InvalidParams(
                "params must be an object".to_string(),
            ));
        }

        if params.get("session_id").and_then(|v| v.as_str()).is_none() {
            return Err(crate::rUvOSError::InvalidParams(
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
                .unwrap()
                .to_string();

            let store = SESSION_STORE.read().unwrap();
            let metadata = store.get(&session_id).cloned();

            if let Some(meta) = metadata {
                Ok(json!({
                    "session_id": session_id,
                    "name": meta.name,
                    "rvf_path": meta.rvf_path,
                    "created_at": meta.created_at,
                    "parent_id": meta.parent_id,
                    "status": "resumed",
                    "found": true
                }))
            } else {
                Ok(json!({
                    "session_id": session_id,
                    "status": "not_found",
                    "found": false,
                    "error": "Session not found"
                }))
            }
        })
    }
}

// ============================================================================
// session.fork handler
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
            return Err(crate::rUvOSError::InvalidParams(
                "params must be an object".to_string(),
            ));
        }

        if params
            .get("source_session_id")
            .and_then(|v| v.as_str())
            .is_none()
        {
            return Err(crate::rUvOSError::InvalidParams(
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
                .unwrap()
                .to_string();

            let new_session_id = Uuid::new_v4().to_string();
            let now = Utc::now().to_rfc3339();

            let store = SESSION_STORE.read().unwrap();
            let source_meta = store.get(&source_session_id).cloned();

            drop(store);

            if let Some(source) = source_meta {
                // Create new metadata with parent tracking
                let forked_rvf_path = format!(".rvf/{}", new_session_id);
                let new_metadata = SessionMetadata {
                    id: SessionId(Uuid::parse_str(&new_session_id).unwrap()),
                    name: source.name.clone().map(|n| format!("{}-fork", n)),
                    rvf_path: forked_rvf_path,
                    created_at: now.clone(),
                    parent_id: Some(source_session_id.clone()),
                };

                let mut store = SESSION_STORE.write().unwrap();
                store.insert(new_session_id.clone(), new_metadata);

                Ok(json!({
                    "forked_id": new_session_id,
                    "source_session_id": source_session_id,
                    "rvf_path": format!(".rvf/{}", new_session_id),
                    "created_at": now,
                    "status": "forked",
                    "success": true
                }))
            } else {
                Ok(json!({
                    "forked_id": None::<String>,
                    "source_session_id": source_session_id,
                    "status": "source_not_found",
                    "success": false,
                    "error": "Source session not found"
                }))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_create() {
        let handler = SessionCreateHandler;
        let params = json!({"name": "test-session"});

        let result = handler.execute(params).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.get("session_id").is_some());
        assert!(response.get("rvf_path").is_some());
        assert!(response.get("created_at").is_some());
        assert_eq!(response.get("status").unwrap(), "created");
    }

    #[tokio::test]
    async fn test_session_create_no_name() {
        let handler = SessionCreateHandler;
        let params = json!({});

        let result = handler.execute(params).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.get("session_id").is_some());
        // When no name is provided, it's serialized as null
        assert!(response.get("name").is_some());
        assert!(response.get("name").unwrap().is_null());
    }

    #[tokio::test]
    async fn test_session_resume_found() {
        let create_handler = SessionCreateHandler;
        let create_result = create_handler.execute(json!({"name": "test"})).await;
        let session_id = create_result
            .unwrap()
            .get("session_id")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        let resume_handler = SessionResumeHandler;
        let resume_result = resume_handler
            .execute(json!({"session_id": session_id.clone()}))
            .await;

        assert!(resume_result.is_ok());
        let response = resume_result.unwrap();
        assert_eq!(response.get("status").unwrap(), "resumed");
        assert_eq!(
            response
                .get("session_id")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            session_id
        );
        assert!(response.get("found").unwrap().as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_session_resume_not_found() {
        let handler = SessionResumeHandler;
        let params = json!({"session_id": "nonexistent-id"});

        let result = handler.execute(params).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.get("status").unwrap(), "not_found");
        assert!(!response.get("found").unwrap().as_bool().unwrap());
    }

    #[tokio::test]
    async fn test_session_fork() {
        let create_handler = SessionCreateHandler;
        let create_result = create_handler.execute(json!({"name": "original"})).await;
        let source_id = create_result
            .unwrap()
            .get("session_id")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        let fork_handler = SessionForkHandler;
        let fork_result = fork_handler
            .execute(json!({"source_session_id": source_id.clone()}))
            .await;

        assert!(fork_result.is_ok());
        let response = fork_result.unwrap();
        assert_eq!(response.get("status").unwrap(), "forked");
        assert!(response.get("success").unwrap().as_bool().unwrap());
        assert!(response.get("forked_id").is_some());
        assert_eq!(
            response
                .get("source_session_id")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            source_id.clone()
        );

        // Verify the forked session can be resumed
        let forked_id = response
            .get("forked_id")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        let resume_handler = SessionResumeHandler;
        let resume_result = resume_handler
            .execute(json!({"session_id": forked_id.clone()}))
            .await;

        assert!(resume_result.is_ok());
        let resume_response = resume_result.unwrap();
        assert_eq!(resume_response.get("status").unwrap(), "resumed");
        assert_eq!(
            resume_response
                .get("parent_id")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string(),
            source_id.clone()
        );
    }

    #[tokio::test]
    async fn test_session_fork_source_not_found() {
        let handler = SessionForkHandler;
        let params = json!({"source_session_id": "nonexistent-source"});

        let result = handler.execute(params).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.get("status").unwrap(), "source_not_found");
        assert!(!response.get("success").unwrap().as_bool().unwrap());
    }

    #[test]
    fn test_session_create_validation() {
        let handler = SessionCreateHandler;

        let invalid_params = json!({"not_an_object": 123});
        let result = handler.validate(&invalid_params);
        assert!(result.is_ok()); // object is still valid

        let params_with_invalid_name = json!({"name": 123});
        let result = handler.validate(&params_with_invalid_name);
        assert!(result.is_err());
    }

    #[test]
    fn test_session_resume_validation() {
        let handler = SessionResumeHandler;

        let invalid_params = json!({});
        let result = handler.validate(&invalid_params);
        assert!(result.is_err());

        let valid_params = json!({"session_id": "some-id"});
        let result = handler.validate(&valid_params);
        assert!(result.is_ok());
    }

    #[test]
    fn test_session_fork_validation() {
        let handler = SessionForkHandler;

        let invalid_params = json!({});
        let result = handler.validate(&invalid_params);
        assert!(result.is_err());

        let valid_params = json!({"source_session_id": "some-id"});
        let result = handler.validate(&valid_params);
        assert!(result.is_ok());
    }
}
