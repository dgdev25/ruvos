//! ruvos-session: real `.rvf` container write/read, fork (COW-branch),
//! and a real SHAKE-256 witness chain (via `rvf-crypto`).
//!
//! An `.rvf` container is a JSON file on disk with two parts:
//! - `payload`: session metadata + a memory snapshot + optional parent link
//! - `witness`: a hex-encoded SHAKE-256 hash-linked witness chain (rvf-crypto
//!   `WITNESS_SEG`). Each entry chains to the previous via `prev_hash`, and the
//!   final entry's `action_hash` attests the exact payload bytes.
//!
//! Verification (a) replays the chain links and (b) checks the last entry's
//! `action_hash` equals a **keyed** HMAC-SHA256 over the current payload — so a
//! party without the signing key can neither tamper with the payload nor forge
//! a fresh chain that attests it (authenticity, not just tamper-evidence).
//! Forking extends the parent's chain, giving real cryptographic lineage.

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::BTreeMap;
use uuid::Uuid;

pub mod fork;
pub mod rvf;
pub mod verify;

pub use fork::fork_session;
pub use rvf::{read_container, read_session, write_container, write_session, RvfContainer};
pub use verify::{verify_container, verify_signature, witness_type_provenance};

/// The signing key for `.rvf` witness attestation.
///
/// Resolution order:
/// 1. `RUVOS_RVF_KEY` env var (≥16 bytes) — for deployments / keystores.
/// 2. A per-install key, randomly generated on first use and persisted with
///    `0600` perms to `<data_root>/.rvf-key` (gitignored). This is a real
///    per-install secret — **not** a hardcoded/committed key — so two different
///    installs cannot forge each other's containers.
pub fn signing_key() -> Vec<u8> {
    // Resolve once per process and cache, so concurrent callers can't race to
    // generate divergent keys.
    static KEY: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
    KEY.get_or_init(|| {
        if let Ok(s) = std::env::var("RUVOS_RVF_KEY") {
            let b = s.into_bytes();
            assert!(
                b.len() >= 16,
                "RUVOS_RVF_KEY must be at least 16 bytes (got {})",
                b.len()
            );
            return b;
        }
        load_or_create_install_key()
    })
    .clone()
}

/// Data root for the install key — honors `RUVOS_HOME`, else `./.ruvos`.
fn key_root() -> std::path::PathBuf {
    std::env::var("RUVOS_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("./.ruvos"))
}

/// Read the per-install key, generating and persisting one on first use.
fn load_or_create_install_key() -> Vec<u8> {
    let path = key_root().join(".rvf-key");
    if let Ok(bytes) = std::fs::read(&path) {
        if bytes.len() >= 16 {
            return bytes;
        }
    }
    // Generate 32 random bytes.
    use rand::RngCore;
    let mut key = vec![0u8; 32];
    rand::thread_rng().fill_bytes(&mut key);

    // Best-effort atomic persist with restrictive permissions. If another
    // process won the race, prefer the key already on disk.
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let tmp = path.with_extension("key.tmp");
    if std::fs::write(&tmp, &key).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        // rename is atomic; only the first writer's key sticks.
        let _ = std::fs::rename(&tmp, &path);
    }
    // Re-read so all racing processes converge on the persisted key.
    match std::fs::read(&path) {
        Ok(bytes) if bytes.len() >= 16 => bytes,
        _ => key,
    }
}

/// Keyed attestation of a payload: HMAC-SHA256(signing_key, canonical_bytes).
/// Output is 32 bytes, matching a witness entry's `action_hash`. Because it is
/// keyed, only a holder of the key can produce a chain that verifies.
pub fn keyed_attestation(payload: &Session) -> [u8; 32] {
    let mut mac =
        <Hmac<Sha256>>::new_from_slice(&signing_key()).expect("HMAC accepts any key length");
    mac.update(&payload.canonical_bytes());
    let bytes = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    out
}

/// Session metadata + state, persisted inside an `.rvf` container.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Session {
    pub id: Uuid,
    pub rvf_path: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    /// Parent session id when this session was forked (COW branch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<Uuid>,
    /// Arbitrary session state / memory snapshot (key -> JSON-encoded value).
    #[serde(default)]
    pub state: BTreeMap<String, String>,
}

impl Session {
    pub fn new() -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4(),
            rvf_path: String::new(),
            name: String::new(),
            created_at: now.clone(),
            updated_at: now,
            parent: None,
            state: BTreeMap::new(),
        }
    }

    /// Canonical bytes used for signing/verifying — stable serialization.
    /// BTreeMap + serde_json gives a deterministic field/key order.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
