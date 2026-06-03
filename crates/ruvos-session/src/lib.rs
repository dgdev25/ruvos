//! ruvos-session: real `.rvf` container write/read, fork (COW-branch),
//! and HMAC-SHA256 signature verification.
//!
//! An `.rvf` container is a JSON file on disk with two parts:
//! - `payload`: session metadata + a memory snapshot + optional parent link
//! - `signature`: a hex HMAC-SHA256 over the canonical payload bytes
//!
//! Verification recomputes the HMAC and compares it, so any tampering with the
//! payload invalidates the container.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

pub mod fork;
pub mod rvf;
pub mod verify;

pub use fork::fork_session;
pub use rvf::{read_session, write_session, RvfContainer};
pub use verify::{sign_payload, verify_container, verify_signature};

/// The signing key for `.rvf` containers. In a real deployment this comes from
/// the environment / a keystore; we fall back to a fixed dev key so containers
/// are still genuinely signed and verifiable in tests and local use.
pub fn signing_key() -> Vec<u8> {
    std::env::var("RUVOS_RVF_KEY")
        .map(|s| s.into_bytes())
        .unwrap_or_else(|_| b"ruvos-default-rvf-signing-key-v4".to_vec())
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
