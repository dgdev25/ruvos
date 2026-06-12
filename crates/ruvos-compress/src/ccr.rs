//! Compressed-content references (CCR): every compressed payload gets a
//! stable short reference under which the original is retrievable.
//!
//! Originals are kept in an in-process cache for fast retrieval AND spilled
//! to disk (`$RUVOS_HOME/compress-originals/<ref>.txt`) so a reference handed
//! to an MCP client survives a server restart. Spilled originals are pruned
//! after `RETENTION_SECS`.

use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

static ORIGINALS: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(|| Mutex::new(HashMap::new()));

const RETENTION_SECS: u64 = 7 * 86_400;

fn short_ref(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    hex::encode(&digest[..12])
}

/// Disk spill directory: `$RUVOS_HOME/compress-originals` (the same root
/// convention ruvos-mcp's `paths::data_root()` resolves to).
fn spill_dir() -> PathBuf {
    std::env::var("RUVOS_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./.ruvos"))
        .join("compress-originals")
}

fn spill_path(reference: &str) -> Option<PathBuf> {
    // References are hex digests; reject anything else so a crafted
    // reference can't traverse out of the spill dir.
    if reference.is_empty() || !reference.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(spill_dir().join(format!("{reference}.txt")))
}

fn prune_spill_dir() {
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(RETENTION_SECS))
        .unwrap_or(std::time::UNIX_EPOCH);
    if let Ok(entries) = std::fs::read_dir(spill_dir()) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.modified().map(|m| m < cutoff).unwrap_or(false) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
}

pub fn store_original(content: &str) -> String {
    let reference = short_ref(content);
    {
        let mut guard = ORIGINALS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.insert(reference.clone(), content.to_string());
    }
    // Best-effort disk spill so the reference survives a process restart.
    if let Some(path) = spill_path(&reference) {
        if std::fs::create_dir_all(spill_dir()).is_ok() {
            let _ = std::fs::write(path, content);
        }
        prune_spill_dir();
    }
    reference
}

pub fn retrieve_original(reference: &str) -> Option<String> {
    {
        let guard = ORIGINALS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(content) = guard.get(reference) {
            return Some(content.clone());
        }
    }
    // Fall back to the disk spill (e.g. after a server restart).
    std::fs::read_to_string(spill_path(reference)?).ok()
}

pub fn original_reference(content: &str) -> String {
    short_ref(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spilled_original_is_retrievable_from_disk() {
        let content = "ccr disk spill test content";
        let reference = store_original(content);
        // Simulate a restart: drop the in-memory copy.
        ORIGINALS
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&reference);
        assert_eq!(retrieve_original(&reference).as_deref(), Some(content));
    }

    #[test]
    fn non_hex_reference_is_rejected() {
        assert!(retrieve_original("../../etc/passwd").is_none());
        assert!(spill_path("..").is_none());
    }
}
