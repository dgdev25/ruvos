//! Process-global [`ruvos_store::Store`] accessor (redb-backed swarm state).
//!
//! redb takes an exclusive process-level file lock, so there may be only one
//! open [`Store`] handle per database path per process. This module enforces
//! that by caching a single, leaked `Store` per resolved data root.
//!
//! In production a single store backs the whole process (rooted at
//! [`crate::paths::data_root`]). Under `cfg(test)` the accessor caches one
//! store per (thread-local) data root, so parallel tests that call
//! [`crate::paths::set_test_root`] never share a redb file — mirroring the
//! isolation pattern in [`crate::safety`].

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use lazy_static::lazy_static;
use ruvos_store::Store;

use crate::paths;

/// Path to the redb database file under the data root.
fn db_path() -> PathBuf {
    paths::data_root().join("store.redb")
}

/// Open a store at the current data root, ensuring the root exists first.
fn open_store() -> Store {
    let _ = paths::ensure_root();
    let path = db_path();
    Store::open(&path.to_string_lossy())
        .expect("ruvos-store: failed to open redb store at data root")
}

lazy_static! {
    /// Maps a resolved db path to its dedicated, leaked `Store` handle so that
    /// at most one redb handle exists per path per process.
    static ref STORES: Mutex<HashMap<PathBuf, &'static Store>> = Mutex::new(HashMap::new());
}

/// Process-global store for the current data root.
///
/// The returned `&'static Store` is shared by every caller resolving to the
/// same data root. `Store` is internally synchronized (redb transactions), so
/// no outer lock is required.
pub fn store() -> &'static Store {
    let key = db_path();
    let mut map = STORES.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(s) = map.get(&key) {
        return s;
    }
    let leaked: &'static Store = Box::leak(Box::new(open_store()));
    map.insert(key, leaked);
    leaked
}
