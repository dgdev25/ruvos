//! ruflo-host: CliHost trait + Claude + Codex adapters for multi-CLI orchestration.
//!
//! Normalized event streams across Claude Code, Codex CLI, and Gemini CLI.

pub mod host;
pub mod adapters;

pub use host::{CliHost, ModelSpec, AgentRequest, AgentEvent};
pub use adapters::{ClaudeHost, CodexHost};
