//! ruflo-mcp: JSON-RPC MCP server with 20 core tools.
//!
//! The tool registry includes:
//! - **memory** (4): search, store, retrieve, list
//! - **session** (3): create, resume, fork
//! - **agent** (3): spawn, status, message
//! - **hooks** (3): pre, post, route
//! - **intel** (2): pattern_search, pattern_store
//! - **plugin** (2): list, invoke
//! - **gov** (2): witness_verify, health
//! - **workflow** (1): run

pub mod error;
pub mod protocol;
pub mod server;
pub mod tools;

pub use error::{RufloError, Result};
pub use protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
pub use server::serve;
pub use tools::{create_registry, tool_registry, ToolRegistry};
