//! Session domain tools (3): create, resume, fork

use super::handler::{ToolHandler, ExecuteFuture};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: SessionId,
    pub rvf_path: String,
    pub created_at: String,
}

// ============================================================================
// Stub handlers for session tools
// ============================================================================

pub struct SessionCreateStub;

impl ToolHandler for SessionCreateStub {
    fn name(&self) -> &'static str {
        "create"
    }

    fn domain(&self) -> &'static str {
        "session"
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }

    fn execute(&self, _params: Value) -> ExecuteFuture {
        Box::pin(async move {
            // TODO: Initialize .rvf container and write metadata
            let id = Uuid::new_v4();
            Ok(json!({
                "session_id": id,
                "rvf_path": format!(".rvf/{}", id),
            }))
        })
    }
}

pub struct SessionResumeStub;

impl ToolHandler for SessionResumeStub {
    fn name(&self) -> &'static str {
        "resume"
    }

    fn domain(&self) -> &'static str {
        "session"
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        // TODO: Validate required field: session_id
        Ok(())
    }

    fn execute(&self, _params: Value) -> ExecuteFuture {
        Box::pin(async move {
            // TODO: Read from .rvf container, restore memory
            Ok(json!({
                "session_id": Uuid::new_v4(),
                "rvf_path": String::new(),
                "created_at": String::new(),
            }))
        })
    }
}

pub struct SessionForkStub;

impl ToolHandler for SessionForkStub {
    fn name(&self) -> &'static str {
        "fork"
    }

    fn domain(&self) -> &'static str {
        "session"
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        // TODO: Validate required field: source_id
        Ok(())
    }

    fn execute(&self, _params: Value) -> ExecuteFuture {
        Box::pin(async move {
            // TODO: Use rvf-cow to fork session
            Ok(json!({
                "forked_id": Uuid::new_v4(),
            }))
        })
    }
}
