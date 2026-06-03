//! Resolves the rUvOS data directory where tools persist real state.
//!
//! Source of truth is disk so state survives process restarts. The root is
//! `$RUVOS_HOME` when set (used by tests to isolate), otherwise `./.ruvos`.

use std::path::PathBuf;

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

/// Path to the JSON-backed agent registry.
pub fn agents_file() -> PathBuf {
    data_root().join("agents.json")
}

/// Ensure the data root exists, returning it.
pub fn ensure_root() -> std::io::Result<PathBuf> {
    let root = data_root();
    std::fs::create_dir_all(&root)?;
    Ok(root)
}
