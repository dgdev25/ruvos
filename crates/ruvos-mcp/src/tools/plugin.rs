//! Plugin domain tools (2): list, invoke

use super::handler::{ExecuteFuture, ToolHandler};
use crate::{error::RuvosError, Result};
use ruvos_plugin_host::{create_discoverer, create_executor, types::ExecutionRequest};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub description: String,
    pub path: String,
}

// ============================================================================
// Real handlers for plugin tools
// ============================================================================

pub struct PluginListHandler;

impl PluginListHandler {
    pub fn new() -> Self {
        PluginListHandler
    }
}

impl Default for PluginListHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for PluginListHandler {
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
            let discoverer = create_discoverer();

            // Try to discover plugins in standard locations
            let mut all_plugins = Vec::new();

            // 1. ./.ruvos/plugins/
            if let Ok(plugins) = discoverer.discover_in_directory(Path::new("./.ruvos/plugins")) {
                all_plugins.extend(plugins);
            }

            // 2. ~/.ruvos/plugins/
            if let Ok(home_dir) = std::env::var("HOME") {
                let home_plugins = Path::new(&home_dir).join(".ruvos/plugins");
                if let Ok(plugins) = discoverer.discover_in_directory(&home_plugins) {
                    all_plugins.extend(plugins);
                }
            }

            // 3. $RUFLO_HOME/plugins/
            if let Ok(ruvos_home) = std::env::var("RUFLO_HOME") {
                let ruvos_plugins = Path::new(&ruvos_home).join("plugins");
                if let Ok(plugins) = discoverer.discover_in_directory(&ruvos_plugins) {
                    all_plugins.extend(plugins);
                }
            }

            // Convert to JSON response
            let plugin_infos: Vec<Value> = all_plugins
                .iter()
                .map(|plugin| {
                    json!({
                        "name": plugin.name,
                        "version": plugin.manifest.plugin.version,
                        "description": plugin.manifest.plugin.description,
                        "path": plugin.path.display().to_string(),
                        "agents": plugin.agents.iter().map(|a| &a.name).collect::<Vec<_>>(),
                        "skills": plugin.skills.iter().map(|s| &s.name).collect::<Vec<_>>(),
                        "commands": plugin.commands.iter().map(|c| &c.name).collect::<Vec<_>>(),
                    })
                })
                .collect();

            let count = plugin_infos.len();

            Ok(json!({
                "plugins": plugin_infos,
                "count": count,
            }))
        })
    }
}

pub struct PluginInvokeHandler;

impl PluginInvokeHandler {
    pub fn new() -> Self {
        PluginInvokeHandler
    }
}

impl Default for PluginInvokeHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for PluginInvokeHandler {
    fn name(&self) -> &'static str {
        "invoke"
    }

    fn domain(&self) -> &'static str {
        "plugin"
    }

    fn validate(&self, params: &Value) -> Result<()> {
        // Validate required fields: plugin_name, command
        if !params.is_object() {
            return Err(RuvosError::ValidationError(
                "Parameters must be a JSON object".to_string(),
            ));
        }

        if params.get("plugin_name").is_none() {
            return Err(RuvosError::ValidationError(
                "Missing required parameter: plugin_name".to_string(),
            ));
        }

        if params.get("command").is_none() {
            return Err(RuvosError::ValidationError(
                "Missing required parameter: command".to_string(),
            ));
        }

        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let plugin_name = match params.get("plugin_name").and_then(|v| v.as_str()) {
                Some(name) => name.to_string(),
                None => {
                    return Ok(json!({
                        "status": 1,
                        "stdout": "",
                        "stderr": "plugin_name must be a string",
                    }))
                }
            };

            let command = match params.get("command").and_then(|v| v.as_str()) {
                Some(cmd) => cmd.to_string(),
                None => {
                    return Ok(json!({
                        "status": 1,
                        "stdout": "",
                        "stderr": "command must be a string",
                    }))
                }
            };

            let args: Vec<String> = params
                .get("args")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();

            // Security: Validate command exists in plugin manifest before execution
            let plugin_found = validate_command_in_plugin(&plugin_name, &command).await;
            if let Err(e) = plugin_found {
                return Ok(json!({
                    "status": 1,
                    "stdout": "",
                    "stderr": e.message(),
                }));
            }

            let executor = create_executor();
            let request = ExecutionRequest {
                plugin_name,
                command,
                args,
                cwd: None,
            };

            match executor.execute(&request).await {
                Ok(result) => Ok(json!({
                    "status": result.status,
                    "stdout": result.stdout,
                    "stderr": result.stderr,
                })),
                Err(e) => Ok(json!({
                    "status": 1,
                    "stdout": "",
                    "stderr": e.to_string(),
                })),
            }
        })
    }
}

/// Validates that a command exists in a plugin's manifest.
/// Searches in standard plugin locations and returns an error if not found.
async fn validate_command_in_plugin(plugin_name: &str, command: &str) -> Result<()> {
    let discoverer = create_discoverer();

    // Locations to search for plugins
    let search_paths = vec![
        Path::new("./.ruvos/plugins").to_path_buf(),
        {
            if let Ok(home) = std::env::var("HOME") {
                Path::new(&home).join(".ruvos/plugins")
            } else {
                Path::new("./.ruvos/plugins").to_path_buf()
            }
        },
        {
            if let Ok(ruvos_home) = std::env::var("RUFLO_HOME") {
                Path::new(&ruvos_home).join("plugins")
            } else {
                Path::new("./.ruvos/plugins").to_path_buf()
            }
        },
    ];

    for path in search_paths {
        if let Ok(plugins) = discoverer.discover_in_directory(&path) {
            if let Some(plugin) = plugins.iter().find(|p| p.name == plugin_name) {
                // Found the plugin, now check if command is in its commands list
                if plugin
                    .commands
                    .iter()
                    .any(|cmd_meta| cmd_meta.name == command)
                {
                    return Ok(());
                } else {
                    // Plugin found but command not in manifest
                    let available_commands: Vec<&str> =
                        plugin.commands.iter().map(|c| c.name.as_str()).collect();
                    return Err(RuvosError::ValidationError(format!(
                        "Command '{}' not found in plugin '{}'. Available commands: {}",
                        command,
                        plugin_name,
                        available_commands.join(", ")
                    )));
                }
            }
        }
    }

    // Plugin not found
    Err(RuvosError::ValidationError(format!(
        "Plugin '{}' not found in any search path",
        plugin_name
    )))
}
