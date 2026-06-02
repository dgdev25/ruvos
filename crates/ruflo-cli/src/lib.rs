//! ruflo-cli: Clap-based CLI shell for the Ruflo orchestration system.
//!
//! Exposes commands like `ruflo init`, `ruflo mcp serve`, and `ruflo agent spawn`.
//! Integrates with `ruflo-mcp` for tool dispatch and `ruflo-host` for multi-CLI orchestration.

pub mod commands;
pub mod dispatch;

pub use commands::{init, mcp};
pub use dispatch::dispatch;
