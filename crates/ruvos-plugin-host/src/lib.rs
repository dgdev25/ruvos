//! ruvos-plugin-host: Plugin discovery (markdown + YAML frontmatter), manifest parsing.
//!
//! Discovery order (first match wins):
//! 1. ./.ruvos/plugins/<name>/
//! 2. ~/.ruvos/plugins/<name>/
//! 3. $RUFLO_HOME/plugins/<name>/
//! 4. <workspace>/crates/ruvos-plugin-host/registry/<name>/

pub mod discover;
pub mod error;
pub mod executor;
pub mod install;
pub mod manifest;
pub mod parser;
pub mod types;

pub use discover::PluginDiscoverer;
pub use error::{PluginError, Result};
pub use executor::PluginExecutor;
pub use types::*;

pub fn create_discoverer() -> PluginDiscoverer {
    PluginDiscoverer
}

pub fn create_executor() -> PluginExecutor {
    PluginExecutor::new()
}
