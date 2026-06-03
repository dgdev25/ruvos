//! `ruvos-memory-graph` — temporal knowledge graph for rUvOS agent memory.
//!
//! Inspired by graphiti's episode/entity/edge model.  Entirely self-contained:
//! no dependency on ruvos-mcp.  Persistence is JSON-on-disk (atomic write via
//! temp + rename).  Embedding uses the same FNV-1a feature-hashing trick as the
//! MCP crate but is duplicated here to avoid a circular dependency.

pub mod edge;
pub mod extract;
pub mod graph;
pub mod graph_embed;
pub mod node;
pub mod persist;

pub use edge::EntityEdge;
pub use graph::MemoryGraph;
pub use node::{EntityNode, Episode};
