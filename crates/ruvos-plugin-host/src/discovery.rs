//! Plugin discovery from filesystem with markdown + YAML frontmatter parsing.

use serde::{Deserialize, Serialize};

/// Plugin manifest parsed from plugin.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub authors: Vec<String>,
    pub path: String,
}

/// Discover all available plugins from filesystem.
pub async fn discover_plugins() -> anyhow::Result<Vec<PluginManifest>> {
    // TODO: Search discovery paths in order:
    // 1. ./.ruvos/plugins/
    // 2. ~/.ruvos/plugins/
    // 3. $RUFLO_HOME/plugins/
    // 4. <workspace>/crates/ruvos-plugin-host/registry/
    // For each directory found, parse plugin.toml and README.md
    Ok(vec![])
}

/// List all plugins with their metadata.
pub async fn list_plugins() -> anyhow::Result<Vec<PluginManifest>> {
    discover_plugins().await
}

/// Invoke a plugin command via shell execution.
pub async fn invoke_plugin(_name: &str, _command: &str) -> anyhow::Result<String> {
    // TODO: Look up manifest, execute shell command via tokio::process
    Ok(String::new())
}
