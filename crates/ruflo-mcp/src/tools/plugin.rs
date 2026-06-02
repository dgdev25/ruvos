//! Plugin domain tools (2): list, invoke

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub description: String,
    pub path: String,
}

/// Installed plugins + skills (discovered from disk).
pub async fn list() -> anyhow::Result<Vec<PluginInfo>> {
    // TODO: Invoke ruflo-plugin-host discovery
    Ok(vec![])
}

/// Run a plugin command (shell exec via tokio).
pub async fn invoke(_plugin_name: &str, _command: &str) -> anyhow::Result<String> {
    // TODO: Look up plugin manifest, execute shell command via tokio::process
    Ok(String::new())
}
