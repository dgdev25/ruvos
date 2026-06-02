//! ruflo-plugin-host: Plugin discovery (markdown + YAML frontmatter), manifest parsing.
//!
//! Discovery order (first match wins):
//! 1. ./.ruflo/plugins/<name>/
//! 2. ~/.ruflo/plugins/<name>/
//! 3. $RUFLO_HOME/plugins/<name>/
//! 4. <workspace>/crates/ruflo-plugin-host/registry/<name>/

pub mod error;
pub mod types;
pub mod manifest;
pub mod parser;
pub mod discover;
pub mod executor;

pub use error::{PluginError, Result};
pub use types::*;
pub use discover::PluginDiscoverer;
pub use executor::PluginExecutor;

pub fn create_discoverer() -> PluginDiscoverer {
    PluginDiscoverer
}

pub fn create_executor() -> PluginExecutor {
    PluginExecutor::new()
}
