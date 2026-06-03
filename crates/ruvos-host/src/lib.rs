//! ruvos-host: CliHost trait + Claude + Codex adapters for multi-CLI orchestration.
//!
//! Normalized event streams across Claude Code, Codex CLI, and Gemini CLI.

pub mod adapters;
pub mod host;

pub use adapters::{ClaudeHost, CodexHost};
pub use host::{AgentEvent, AgentRequest, CliError, CliHost, ModelSpec, ToolCall, ToolResponse};
