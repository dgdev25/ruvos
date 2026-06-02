//! Plugin domain tools (2): list, invoke

use super::handler::{ExecuteFuture, ToolHandler};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub description: String,
    pub path: String,
}

// ============================================================================
// Stub handlers for plugin tools
// ============================================================================

pub struct PluginListStub;

impl ToolHandler for PluginListStub {
    fn name(&self) -> &'static str {
        "list"
    }

    fn domain(&self) -> &'static str {
        "plugin"
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }

    fn execute(&self, _params: Value) -> ExecuteFuture {
        Box::pin(async move {
            // TODO: Invoke ruflo-plugin-host discovery
            Ok(json!({
                "plugins": [],
            }))
        })
    }
}

pub struct PluginInvokeStub;

impl ToolHandler for PluginInvokeStub {
    fn name(&self) -> &'static str {
        "invoke"
    }

    fn domain(&self) -> &'static str {
        "plugin"
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        // TODO: Validate required fields: plugin_name, command
        Ok(())
    }

    fn execute(&self, _params: Value) -> ExecuteFuture {
        Box::pin(async move {
            // TODO: Look up plugin manifest, execute shell command via tokio::process
            Ok(json!({
                "output": "",
            }))
        })
    }
}
