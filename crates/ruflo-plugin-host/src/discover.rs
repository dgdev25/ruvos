//! Discover stub (implemented in Task 4)

use crate::error::Result;
use std::path::Path;

/// Placeholder for PluginDiscoverer (implemented in Task 4)
#[derive(Debug, Default)]
pub struct PluginDiscoverer;

impl PluginDiscoverer {
    pub fn discover_in_directory(&self, _dir: &Path) -> Result<Vec<crate::types::Plugin>> {
        Ok(Vec::new())
    }
}
