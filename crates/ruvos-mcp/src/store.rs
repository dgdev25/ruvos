//! Transient [`ruvos_store::Store`] access (redb-backed swarm state).
//!
//! redb takes an **exclusive process-level file lock** for as long as a `Store`
//! handle is open, so a long-lived handle would stop any second rUvOS instance
//! sharing the same `$RUVOS_HOME` from opening the store at all (it would error
//! with "Database already open").
//!
//! To let multiple instances share one data root, we open the store **only for
//! the duration of a single operation** and drop it immediately, releasing the
//! lock. Concurrent instances therefore interleave; the brief open contention
//! window is handled with a short bounded retry. Opening never panics — if the
//! store genuinely can't be acquired, callers receive `None` and degrade
//! gracefully (relay audit is skipped; agent tools return a clear "store busy"
//! error) rather than crashing the MCP server.
//!
//! The rUvOS MCP server processes one request at a time over stdio, so there is
//! no intra-process concurrency on the store; contention is only ever between
//! separate instances, which the retry resolves.

use std::path::PathBuf;
use std::time::Duration;

use ruvos_store::Store;

use crate::constants::{STORE_BASE_DELAY_MS, STORE_MAX_TRIES};
use crate::paths;

/// Path to the redb database file under the current data root.
fn db_path() -> PathBuf {
    paths::data_root().join("store.redb")
}

/// True if an open error is a transient cross-instance lock conflict (vs. a real
/// corruption/IO error).
fn is_lock_conflict(err: &anyhow::Error) -> bool {
    let msg = err.to_string().to_lowercase();
    msg.contains("already open") || msg.contains("acquire lock") || msg.contains("locked")
}

/// Open a fresh, owned `Store` at the current data root, or `None` if it cannot
/// be acquired. Retries briefly on cross-instance lock contention. The returned
/// `Store` releases the redb lock as soon as it is dropped, so callers should
/// keep it only for the operation at hand.
pub fn try_store() -> Option<Store> {
    let _ = paths::ensure_root();
    let path = db_path().to_string_lossy().into_owned();

    for attempt in 0..STORE_MAX_TRIES {
        match Store::open(&path) {
            Ok(s) => return Some(s),
            Err(e) if is_lock_conflict(&e) && attempt + 1 < STORE_MAX_TRIES => {
                std::thread::sleep(Duration::from_millis(STORE_BASE_DELAY_MS));
                continue;
            }
            Err(e) => {
                tracing::debug!("ruvos-store unavailable at {}: {}", path, e);
                return None;
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_and_reopen_releases_lock() {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(dir.path().to_path_buf());
        // First handle opens; dropping it must release the lock so a second
        // transient open succeeds (this is what lets instances interleave).
        {
            let s = try_store().expect("first open");
            drop(s);
        }
        let s2 = try_store().expect("second open after release");
        drop(s2);
    }
}
