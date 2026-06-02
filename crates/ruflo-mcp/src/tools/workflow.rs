//! Workflow domain tools (1): run

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRequest {
    pub workflow_type: String, // feature / bugfix / refactor / security
    pub host: String,
    pub archetype: String,
    pub task: String,
}

/// Execute an orchestration template.
pub async fn run(_request: WorkflowRequest) -> anyhow::Result<String> {
    // TODO: Dispatch to ruflo-host with workflow orchestration
    Ok(String::new())
}
