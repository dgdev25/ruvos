//! JSON persistence for the memory graph.
//!
//! Atomic write pattern: serialize to a `.tmp` sidecar, then `rename` over the
//! real file — same approach used throughout rUvOS for crash safety.

use crate::edge::EntityEdge;
use crate::node::{EntityNode, Episode};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The complete on-disk representation of a `MemoryGraph`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GraphStore {
    pub nodes: Vec<EntityNode>,
    pub edges: Vec<EntityEdge>,
    pub episodes: Vec<Episode>,
}

/// Load graph state from `path`.  Returns a default (empty) store if the file
/// does not yet exist.
pub fn load(path: &Path) -> Result<GraphStore> {
    match std::fs::read(path) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).with_context(|| format!("parsing graph file {path:?}"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(GraphStore::default()),
        Err(e) => Err(e).with_context(|| format!("reading graph file {path:?}")),
    }
}

/// Atomically persist `store` to `path`.
pub fn save(path: &Path, store: &GraphStore) -> Result<()> {
    // Ensure parent directory exists.
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating parent dir {parent:?}"))?;
    }

    let bytes = serde_json::to_vec_pretty(store).context("serialising graph store")?;

    let tmp: PathBuf = path.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes).with_context(|| format!("writing temp file {tmp:?}"))?;
    std::fs::rename(&tmp, path).with_context(|| format!("committing graph file {path:?}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::EntityNode;

    #[test]
    fn roundtrip_empty_store() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("graph.json");
        let store = GraphStore::default();
        save(&p, &store).unwrap();
        let loaded = load(&p).unwrap();
        assert_eq!(loaded.nodes.len(), 0);
        assert_eq!(loaded.edges.len(), 0);
    }

    #[test]
    fn roundtrip_with_node() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("graph.json");
        let mut store = GraphStore::default();
        store.nodes.push(EntityNode::new("Alice"));
        save(&p, &store).unwrap();
        let loaded = load(&p).unwrap();
        assert_eq!(loaded.nodes.len(), 1);
        assert_eq!(loaded.nodes[0].name, "Alice");
    }

    #[test]
    fn missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("nonexistent.json");
        let store = load(&p).unwrap();
        assert_eq!(store.nodes.len(), 0);
    }
}
