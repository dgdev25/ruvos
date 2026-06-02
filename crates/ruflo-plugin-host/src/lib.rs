//! ruflo-plugin-host: Plugin discovery (markdown + YAML frontmatter), manifest parsing.
//!
//! Discovery order (first match wins):
//! 1. ./.ruflo/plugins/<name>/
//! 2. ~/.ruflo/plugins/<name>/
//! 3. $RUFLO_HOME/plugins/<name>/
//! 4. <workspace>/crates/ruflo-plugin-host/registry/<name>/

pub mod discovery;
pub mod registry;

pub use discovery::{discover_plugins, PluginManifest};
pub use registry::PluginRegistry;
