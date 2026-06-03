//! Plugin registry management.

use crate::discovery::PluginManifest;
use std::collections::HashMap;

/// Plugin registry holding loaded manifests.
#[derive(Debug, Clone)]
pub struct PluginRegistry {
    plugins: HashMap<String, PluginManifest>,
}

impl PluginRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    /// Register a plugin.
    pub fn register(&mut self, manifest: PluginManifest) {
        self.plugins.insert(manifest.name.clone(), manifest);
    }

    /// Look up a plugin by name.
    pub fn get(&self, name: &str) -> Option<&PluginManifest> {
        self.plugins.get(name)
    }

    /// List all registered plugins.
    pub fn list(&self) -> Vec<&PluginManifest> {
        self.plugins.values().collect()
    }

    /// Load all discovered plugins into the registry.
    pub async fn load_all() -> anyhow::Result<Self> {
        let mut registry = Self::new();
        let manifests = crate::discovery::discover_plugins().await?;
        for manifest in manifests {
            registry.register(manifest);
        }
        Ok(registry)
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}
