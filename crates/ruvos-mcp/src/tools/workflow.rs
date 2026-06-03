//! Workflow domain tools (1): run

use super::handler::{ExecuteFuture, ToolHandler};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRequest {
    pub workflow_type: String, // feature / bugfix / refactor / security
    pub host: String,
    pub archetype: String,
    pub task: String,
}

// ============================================================================
// Stub handler for workflow tools
// ============================================================================

pub struct WorkflowRunStub;

impl ToolHandler for WorkflowRunStub {
    fn name(&self) -> &'static str {
        "run"
    }

    fn domain(&self) -> &'static str {
        "workflow"
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        // TODO: Validate required fields: workflow_type, host, archetype, task
        Ok(())
    }

    fn execute(&self, _params: Value) -> ExecuteFuture {
        Box::pin(async move {
            // TODO: Dispatch to ruvos-host with workflow orchestration
            Ok(json!({
                "workflow_id": "",
                "status": "started",
            }))
        })
    }
}
