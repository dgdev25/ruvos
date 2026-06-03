//! ruvos-cli: Clap-based CLI shell for the rUvOS orchestration system.
//!
//! Exposes commands like `ruflo init`, `ruflo mcp serve`, and `ruflo agent spawn`.
//! Integrates with `ruvos-mcp` for tool dispatch and `ruvos-host` for multi-CLI orchestration.

pub mod commands;
pub mod dispatch;

pub use commands::{init, mcp};
pub use dispatch::dispatch;
