use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub plugin: PluginInfo,
    pub capabilities: PluginCapabilities,
    #[serde(default)]
    pub compat: PluginCompat,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub license: String,
    pub authors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCapabilities {
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub hooks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginCompat {
    pub ruflo_min: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub name: String,
    pub path: std::path::PathBuf,
    pub manifest: PluginManifest,
    pub agents: Vec<AgentMetadata>,
    pub skills: Vec<SkillMetadata>,
    pub commands: Vec<CommandMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMetadata {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandMetadata {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginListResponse {
    pub plugins: Vec<PluginInfo>,
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub plugin_name: String,
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionResult {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}
