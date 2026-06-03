//! Signed `.rvf` snapshot/restore for the store.
//!
//! A snapshot is a JSON envelope `{ version, payload, witness }` where:
//! - `payload` is the full [`StoreSnapshot`] (every record in the store), and
//! - `witness` is a hex-encoded SHAKE-256 witness chain (rvf-crypto
//!   `WITNESS_SEG`) whose single entry's `action_hash` is a **keyed**
//!   HMAC-SHA256 over the canonical payload bytes.
//!
//! Verification (a) replays the witness chain links and (b) checks the final
//! entry's `action_hash` equals the keyed attestation of the loaded payload.
//! Because the attestation is keyed, a party without the signing key can
//! neither tamper with the payload nor forge a fresh chain that attests it
//! (authenticity, not just tamper-evidence). This mirrors the pattern used by
//! `ruvos-session`.

use crate::records::StoreSnapshot;
use hmac::{Hmac, Mac};
use rvf_crypto::{create_witness_chain, verify_witness_chain, WitnessEntry};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

/// Witness type byte for provenance entries (rvf-crypto convention: 0x01).
const WITNESS_TYPE_PROVENANCE: u8 = 0x01;

/// On-disk `.rvf` snapshot container.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotContainer {
    pub version: String,
    pub payload: StoreSnapshot,
    /// Hex-encoded SHAKE-256 witness chain (rvf-crypto WITNESS_SEG).
    pub witness: String,
}

/// Canonical bytes used for signing/verifying a snapshot payload.
///
/// `StoreSnapshot` ordering is deterministic for a given store state (the
/// store emits records in a stable key order), so serde_json yields stable
/// bytes suitable for keyed attestation.
fn canonical_bytes(payload: &StoreSnapshot) -> Vec<u8> {
    serde_json::to_vec(payload).unwrap_or_default()
}

/// Keyed attestation of a payload: HMAC-SHA256(signing_key, canonical_bytes).
///
/// Reuses `rvf_crypto`'s signing-key resolution via `ruvos-session` is not
/// possible here without a dependency cycle, so the key is resolved directly
/// the same way: `RUVOS_RVF_KEY` env var if set (>=16 bytes), else a
/// per-install key persisted under `RUVOS_HOME`/`./.ruvos`.
fn keyed_attestation(payload: &StoreSnapshot) -> [u8; 32] {
    let key = signing_key();
    let mut mac = <Hmac<Sha256>>::new_from_slice(&key).expect("HMAC accepts any key length");
    mac.update(&canonical_bytes(payload));
    let bytes = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    out
}

/// The signing key for `.rvf` witness attestation.
///
/// Resolution order (matches `ruvos-session`):
/// 1. `RUVOS_RVF_KEY` env var (>=16 bytes).
/// 2. A per-install key, randomly generated on first use and persisted with
///    `0600` perms to `<data_root>/.rvf-key` (gitignored).
///
/// Sharing the same `.rvf-key` file as `ruvos-session` means snapshots and
/// session containers are attested under one install identity.
fn signing_key() -> Vec<u8> {
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
    // Generate 32 bytes of entropy without pulling in `rand`: use getrandom via
    // uuid v4 mixing, repeated, so we stay within the declared dep set.
    let mut key = Vec::with_capacity(32);
    while key.len() < 32 {
        key.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    }
    key.truncate(32);

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
        let _ = std::fs::rename(&tmp, &path);
    }
    match std::fs::read(&path) {
        Ok(bytes) if bytes.len() >= 16 => bytes,
        _ => key,
    }
}

/// Nanosecond UNIX timestamp for a witness entry.
fn now_ns() -> u64 {
    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0).max(0) as u64
}

/// Build a signed container around a snapshot payload.
pub fn seal(payload: StoreSnapshot) -> SnapshotContainer {
    let entry = WitnessEntry {
        prev_hash: [0u8; 32],
        action_hash: keyed_attestation(&payload),
        timestamp_ns: now_ns(),
        witness_type: WITNESS_TYPE_PROVENANCE,
    };
    SnapshotContainer {
        version: "rvf-store-1".to_string(),
        payload,
        witness: hex::encode(create_witness_chain(&[entry])),
    }
}

/// Verify a container: replay the chain and check the keyed attestation.
pub fn verify(container: &SnapshotContainer) -> bool {
    let chain = match hex::decode(&container.witness) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let entries = match verify_witness_chain(&chain) {
        Ok(e) => e,
        Err(_) => return false,
    };
    let last = match entries.last() {
        Some(e) => e,
        None => return false,
    };
    use hmac::digest::CtOutput;
    let expected = keyed_attestation(&container.payload);
    let lhs = CtOutput::<Hmac<Sha256>>::new(last.action_hash.into());
    let rhs = CtOutput::<Hmac<Sha256>>::new(expected.into());
    lhs == rhs
}

/// Serialize and atomically write a signed snapshot to `path`.
pub fn write_to(container: &SnapshotContainer, path: &str) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(container)?;
    if let Some(parent) = std::path::Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let tmp = format!("{path}.tmp");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Read and verify a signed snapshot from `path`.
pub fn read_from(path: &str) -> anyhow::Result<StoreSnapshot> {
    let bytes = std::fs::read(path)?;
    let container: SnapshotContainer = serde_json::from_slice(&bytes)?;
    if !verify(&container) {
        anyhow::bail!("witness verification failed for {path}");
    }
    Ok(container.payload)
}
