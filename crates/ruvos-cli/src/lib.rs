//! ruvos-cli: Clap-based CLI shell for the rUvOS orchestration system.
//!
//! Exposes commands like `ruvos init`, `ruvos mcp serve`, and `ruvos agent spawn`.
//! Integrates with `ruvos-mcp` for tool dispatch and `ruvos-host` for multi-CLI orchestration.

pub mod commands;
pub mod dispatch;

pub use commands::{init, mcp};
pub use dispatch::dispatch;
