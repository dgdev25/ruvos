//! Plugin discovery: traverse directories, load manifests, parse agents/skills/commands.

use crate::error::{PluginError, Result};
use crate::manifest::read_manifest_from_file;
use crate::parser::parse_frontmatter;
use crate::types::{AgentMetadata, CommandMetadata, Plugin, SkillMetadata};
use std::fs;
use std::path::Path;

/// Discovers plugins in a directory tree by looking for plugin.toml files.
#[derive(Debug, Default)]
pub struct PluginDiscoverer;

impl PluginDiscoverer {
    /// Discover all plugins in a directory.
    ///
    /// Traverses the directory looking for subdirectories containing plugin.toml files.
    /// For each plugin found, loads the manifest and discovers agents, skills, and commands.
    pub fn discover_in_directory(&self, dir: &Path) -> Result<Vec<Plugin>> {
        if !dir.is_dir() {
            return Err(PluginError::InvalidDirectory(format!(
                "path is not a directory: {}",
                dir.display()
            )));
        }

        let mut plugins = Vec::new();

        // Iterate over directory entries
        for entry in fs::read_dir(dir).map_err(PluginError::Io)? {
            let entry = entry.map_err(PluginError::Io)?;
            let path = entry.path();

            // Check if this is a directory
            if path.is_dir() {
                // Check if plugin.toml exists in this directory
                let manifest_path = path.join("plugin.toml");
                if manifest_path.exists() {
                    if let Ok(plugin) = self.load_plugin(&path) {
                        plugins.push(plugin);
                    }
                }
            }
        }

        Ok(plugins)
    }

    /// Load a single plugin from a directory.
    fn load_plugin(&self, plugin_dir: &Path) -> Result<Plugin> {
        let manifest_path = plugin_dir.join("plugin.toml");
        let manifest = read_manifest_from_file(&manifest_path)?;

        let name = manifest.plugin.name.clone();
        let agents = self.load_agents(plugin_dir)?;
        let skills = self.load_skills(plugin_dir)?;
        let commands = self.load_commands(plugin_dir)?;

        Ok(Plugin {
            name,
            path: plugin_dir.to_path_buf(),
            manifest,
            agents,
            skills,
            commands,
        })
    }

    /// Load agent metadata from the agents/ directory.
    fn load_agents(&self, plugin_dir: &Path) -> Result<Vec<AgentMetadata>> {
        self.load_metadata_from_dir(plugin_dir, "agents")
            .and_then(|items| {
                items
                    .into_iter()
                    .map(|(name, description)| Ok(AgentMetadata { name, description }))
                    .collect()
            })
    }

    /// Load skill metadata from the skills/ directory.
    fn load_skills(&self, plugin_dir: &Path) -> Result<Vec<SkillMetadata>> {
        self.load_metadata_from_dir(plugin_dir, "skills")
            .and_then(|items| {
                items
                    .into_iter()
                    .map(|(name, description)| Ok(SkillMetadata { name, description }))
                    .collect()
            })
    }

    /// Load command metadata from the commands/ directory, including the
    /// declared `exec` entrypoint and fixed `args` from frontmatter.
    fn load_commands(&self, plugin_dir: &Path) -> Result<Vec<CommandMetadata>> {
        let commands_dir = plugin_dir.join("commands");
        if !commands_dir.is_dir() {
            return Ok(Vec::new());
        }

        let mut commands = Vec::new();
        for entry in fs::read_dir(&commands_dir).map_err(PluginError::Io)? {
            let entry = entry.map_err(PluginError::Io)?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            let Ok(meta) = parse_frontmatter(&content) else {
                continue;
            };
            let exec = meta
                .metadata
                .get("exec")
                .and_then(|v| v.as_str())
                .map(String::from);
            let exec_args = meta
                .metadata
                .get("args")
                .and_then(|v| v.as_sequence())
                .map(|seq| {
                    seq.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            commands.push(CommandMetadata {
                name: meta.name,
                description: meta.description,
                exec,
                exec_args,
            });
        }
        Ok(commands)
    }

    /// Load metadata from a subdirectory containing markdown files with frontmatter.
    ///
    /// Returns a vector of (name, description) pairs extracted from YAML frontmatter.
    fn load_metadata_from_dir(
        &self,
        plugin_dir: &Path,
        subdir: &str,
    ) -> Result<Vec<(String, String)>> {
        let metadata_dir = plugin_dir.join(subdir);

        // If directory doesn't exist, return empty list
        if !metadata_dir.is_dir() {
            return Ok(Vec::new());
        }

        let mut items = Vec::new();

        for entry in fs::read_dir(&metadata_dir).map_err(PluginError::Io)? {
            let entry = entry.map_err(PluginError::Io)?;
            let path = entry.path();

            // Only process .md files
            if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
                match fs::read_to_string(&path).map_err(PluginError::Io) {
                    Ok(content) => {
                        if let Ok(meta) = parse_frontmatter(&content) {
                            items.push((meta.name, meta.description));
                        }
                    }
                    Err(_) => {
                        // Skip files that can't be read
                    }
                }
            }
        }

        Ok(items)
    }
}
