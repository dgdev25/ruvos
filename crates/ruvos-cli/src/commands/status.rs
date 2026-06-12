//! `ruvos status` — human-facing system-state view (read-only).
//!
//! Reuses the exact handlers behind the gov/swarm/agent/relay MCP tools so
//! the CLI and the MCP surface can never disagree about system state.

use anyhow::Result;
use ruvos_mcp::tools::handler::ToolHandler;
use serde_json::{json, Value};

/// Run one tool handler, mapping a handler error to a JSON error marker so a
/// broken store degrades one section instead of killing the whole view.
async fn section(handler: &dyn ToolHandler, params: Value) -> Value {
    match handler.execute(params).await {
        Ok(v) => v,
        Err(e) => json!({"error": e.message()}),
    }
}

/// Collect the merged system-state view from the gov/swarm/agent/relay
/// handlers. Each section degrades independently on error.
pub async fn collect_status() -> Result<Value> {
    use ruvos_mcp::tools::{agent, gov, relay};
    Ok(json!({
        "health": section(&gov::GovHealthHandler, json!({})).await,
        "swarm": section(&gov::GovSwarmStatusHandler, json!({})).await,
        "agents": section(&agent::AgentStatusHandler, json!({})).await,
        "events": section(&gov::GovEventsHandler, json!({"limit": 10})).await,
        "relays": section(&relay::RelayListHandler, json!({})).await,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn collect_returns_all_sections_on_empty_state() {
        let _guard = crate::commands::ruvos_home_lock().await;
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("RUVOS_HOME", dir.path());
        let v = collect_status().await.unwrap();
        assert!(v.get("health").is_some());
        assert!(v.get("swarm").is_some());
        assert!(v.get("agents").is_some());
        assert!(v.get("events").is_some());
        assert!(v.get("relays").is_some());
    }
}
