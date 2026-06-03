//! Process-global [`SafetyEngine`] shared by the `hooks.pre` risk-assessment
//! path and the `gov.health` introspection path.
//!
//! The engine persists to `<data_root>/safety/safety.json`. In production a
//! single process-global engine is used (initialised from
//! [`crate::paths::data_root`]). Under `cfg(test)` the accessor re-derives a
//! fresh engine from the current thread's (possibly overridden) data root so
//! that parallel tests stay isolated — mirroring the thread-local data-root
//! override in [`crate::paths`].

use std::sync::Mutex;

use lazy_static::lazy_static;
use ruvos_safety::SafetyEngine;

use crate::paths;

/// Sub-path under the rUvOS data root where the safety engine persists.
const SAFETY_SUBDIR: &str = "safety";

fn safety_dir() -> std::path::PathBuf {
    paths::data_root().join(SAFETY_SUBDIR)
}

fn new_engine() -> SafetyEngine {
    let dir = safety_dir();
    // Best-effort: ensure the directory exists so the engine can persist.
    let _ = std::fs::create_dir_all(&dir);
    SafetyEngine::new(&dir.to_string_lossy())
}

#[cfg(not(test))]
lazy_static! {
    static ref ENGINE: Mutex<SafetyEngine> = Mutex::new(new_engine());
}

/// Process-global safety engine guarded by a [`Mutex`].
///
/// Both `hooks.pre` and `gov.health` go through this accessor so they observe
/// and update the same constraint state.
#[cfg(not(test))]
pub fn engine() -> &'static Mutex<SafetyEngine> {
    &ENGINE
}

// ---------------------------------------------------------------------------
// Test-only accessor: a fresh engine per (thread-local) data root.
// ---------------------------------------------------------------------------

#[cfg(test)]
lazy_static! {
    /// Maps a data-root path to its dedicated engine so parallel tests that set
    /// distinct `set_test_root` values never share constraint state.
    static ref ENGINES: Mutex<std::collections::HashMap<std::path::PathBuf, &'static Mutex<SafetyEngine>>> =
        Mutex::new(std::collections::HashMap::new());
}

#[cfg(test)]
pub fn engine() -> &'static Mutex<SafetyEngine> {
    let key = safety_dir();
    let mut map = ENGINES.lock().unwrap();
    if let Some(e) = map.get(&key) {
        return e;
    }
    // Leak a per-root engine so it satisfies the `'static` return contract.
    let leaked: &'static Mutex<SafetyEngine> = Box::leak(Box::new(Mutex::new(new_engine())));
    map.insert(key, leaked);
    leaked
}
