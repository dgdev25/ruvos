//! `.rvf` witness-chain verification (real SHAKE-256 chain via rvf-crypto).

use crate::RvfContainer;
use rvf_crypto::{shake256_256, verify_witness_chain};

/// Witness type byte for provenance entries (rvf-crypto convention: 0x01).
pub const fn witness_type_provenance() -> u8 {
    0x01
}

/// Verify a loaded container:
/// 1. The witness chain replays correctly (each `prev_hash` matches the
///    SHAKE-256 of the preceding entry).
/// 2. The final entry's `action_hash` equals SHAKE-256 of the current payload,
///    proving the chain attests *this exact* payload.
pub fn verify_container(container: &RvfContainer) -> bool {
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
    last.action_hash == shake256_256(&container.payload.canonical_bytes())
}

/// Verify the witness chain of an `.rvf` container on disk.
pub async fn verify_signature(rvf_path: &str) -> anyhow::Result<bool> {
    let bytes = tokio::fs::read(rvf_path).await?;
    let container: RvfContainer = serde_json::from_slice(&bytes)?;
    Ok(verify_container(&container))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rvf::build_chain;
    use crate::Session;
    use rvf_crypto::WitnessEntry;

    #[test]
    fn chain_attests_payload_and_detects_tampering() {
        let mut s = Session::new();
        s.name = "demo".into();
        s.state.insert("k".into(), "\"v\"".into());

        // Genesis chain attesting the payload.
        let entry = WitnessEntry {
            prev_hash: [0u8; 32],
            action_hash: shake256_256(&s.canonical_bytes()),
            timestamp_ns: 1,
            witness_type: witness_type_provenance(),
        };
        let container = RvfContainer {
            version: "rvf-1".into(),
            payload: s.clone(),
            witness: hex::encode(build_chain(&[entry])),
        };
        assert!(verify_container(&container), "valid container must verify");

        // Tamper the payload — chain no longer attests it.
        let mut tampered = container.clone();
        tampered.payload.name = "evil".into();
        assert!(!verify_container(&tampered), "tampered payload must fail");
    }
}
