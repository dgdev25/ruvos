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
//! `action_hash` equals SHAKE-256 of the current payload — so tampering with
//! either the payload or any chain entry is detected. Forking extends the
//! parent's chain, giving real cryptographic lineage.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

pub mod fork;
pub mod rvf;
pub mod verify;

pub use fork::fork_session;
pub use rvf::{read_container, read_session, write_container, write_session, RvfContainer};
pub use verify::{verify_container, verify_signature, witness_type_provenance};

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
