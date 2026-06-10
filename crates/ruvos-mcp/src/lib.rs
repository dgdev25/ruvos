//! ruvos-mcp: JSON-RPC MCP server.
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
//! - **compress** (1): run

mod compress_learning;
pub mod constants;
pub mod daemon;
pub mod error;
pub mod eval;
pub mod llm;
pub mod llm_router;
pub mod math;
pub mod paths;
pub mod protocol;
pub mod rate_limiter;
pub mod relay;
pub mod runtime;
pub mod safety;
pub mod sandbox;
pub mod server;
pub mod server_rmcp;
pub mod skills;
pub mod store;
pub mod swarm;
pub mod task_queue;
pub mod tools;

pub use error::{Result, RuvosError};
pub use protocol::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};
pub use server::JsonRpcServer;
pub use server_rmcp::serve;
pub use tools::{create_registry, tool_registry, ToolRegistry};
