//! Resolves the rUvOS data directory where tools persist real state.
//!
//! Source of truth is disk so state survives process restarts. The root is
//! `$RUVOS_HOME` when set (used by tests to isolate), otherwise `./.ruvos`.

use std::path::{Path, PathBuf};

const BUNDLED_SKILLS_PACK: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/skills/public/skills.redb"
));

#[cfg(test)]
thread_local! {
    /// Per-thread override so parallel tests can isolate their data dir without
    /// racing on the process-global `RUVOS_HOME` env var.
    static ROOT_OVERRIDE: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// Test-only: pin the data root for the current thread (and its current-thread
/// tokio runtime). Each `#[tokio::test]` runs on its own thread, so this gives
/// each test a private data directory.
#[cfg(test)]
pub fn set_test_root(path: PathBuf) {
    ROOT_OVERRIDE.with(|c| *c.borrow_mut() = Some(path));
}

/// Root rUvOS data directory.
pub fn data_root() -> PathBuf {
    #[cfg(test)]
    if let Some(p) = ROOT_OVERRIDE.with(|c| c.borrow().clone()) {
        return p;
    }
    std::env::var("RUVOS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./.ruvos"))
}

/// Directory holding `.rvf` session containers.
pub fn sessions_dir() -> PathBuf {
    data_root().join("rvf")
}

/// Path to the JSON-backed memory store.
pub fn memory_file() -> PathBuf {
    data_root().join("memory.json")
}

/// Path to the JSON-backed intel trajectory store.
pub fn intel_file() -> PathBuf {
    data_root().join("intel.json")
}

/// Path to the JSON-backed intent memory store.
pub fn intent_file() -> PathBuf {
    data_root().join("intent.json")
}

/// Path to the JSON-backed memory-retrieval bandit reward store.
pub fn memory_rewards_file() -> PathBuf {
    data_root().join("memory-rewards.json")
}

/// Path to the JSON-backed agent registry.
pub fn agents_file() -> PathBuf {
    data_root().join("agents.json")
}

/// Path to the JSON-backed temporal memory knowledge graph.
pub fn memory_graph_file() -> PathBuf {
    data_root().join("memory-graph.json")
}

/// Directory holding cross-instance relay presence records + inboxes.
pub fn relays_dir() -> PathBuf {
    data_root().join("relays")
}

/// Path to the JSON-backed coordination contract store.
pub fn coordination_file() -> PathBuf {
    relays_dir().join("contracts.json")
}

/// Path to the JSON-backed swarm state store.
pub fn swarm_file() -> PathBuf {
    data_root().join("swarm.json")
}

/// Path to the portable skills pack.
pub fn skills_pack_file() -> PathBuf {
    data_root().join("skills.redb")
}

/// Ensure the data root exists, returning it.
pub fn ensure_root() -> std::io::Result<PathBuf> {
    let root = data_root();
    std::fs::create_dir_all(&root)?;
    ensure_skills_pack(&root)?;
    Ok(root)
}

fn ensure_skills_pack(root: &Path) -> std::io::Result<()> {
    let pack = root.join("skills.redb");
    if pack.exists() {
        return Ok(());
    }
    std::fs::write(pack, BUNDLED_SKILLS_PACK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_root_bootstraps_bundled_skills_pack() {
        let root = std::env::temp_dir().join(format!("ruvos-paths-{}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        set_test_root(root.clone());

        let resolved = ensure_root().unwrap();

        assert_eq!(resolved, root);
        assert!(resolved.join("skills.redb").exists());
    }
}
