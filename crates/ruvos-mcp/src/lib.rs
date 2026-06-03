//! ruvos-mcp: JSON-RPC MCP server with 24 core tools.
//!
//! The tool registry includes:
//! - **memory** (4): search, store, retrieve, list
//! - **session** (3): create, resume, fork
//! - **agent** (3): spawn, status, message
//! - **hooks** (3): pre, post, route
//! - **intel** (2): pattern_search, pattern_store
//! - **plugin** (2): list, invoke
//! - **gov** (3): witness_verify, health, events
//! - **relay** (3): announce, list, send
//! - **orchestrate** (1): run

pub mod error;
pub mod paths;
pub mod protocol;
pub mod relay;
pub mod safety;
pub mod server;
pub mod store;
pub mod tools;

pub use error::{Result, RuvosError};
pub use protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
pub use server::{serve, JsonRpcServer};
pub use tools::{create_registry, tool_registry, ToolRegistry};
