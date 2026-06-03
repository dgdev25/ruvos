//! `.rvf` container write/read operations — real witness-chained files on disk.

use crate::verify::{verify_container, witness_type_provenance};
use crate::{keyed_attestation, Session};
use rvf_crypto::{create_witness_chain, verify_witness_chain, WitnessEntry};
use serde::{Deserialize, Serialize};

/// On-disk `.rvf` container: a witness-chained envelope around a session payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RvfContainer {
    pub version: String,
    pub payload: Session,
    /// Hex-encoded SHAKE-256 witness chain (rvf-crypto WITNESS_SEG).
    pub witness: String,
}

/// Build a serialized witness chain from ordered entries (re-links prev_hashes).
pub fn build_chain(entries: &[WitnessEntry]) -> Vec<u8> {
    create_witness_chain(entries)
}

/// Nanosecond UNIX timestamp for a witness entry.
fn now_ns() -> u64 {
    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0).max(0) as u64
}

/// A provenance entry attesting `payload`. `prev_hash` is overwritten by
/// `create_witness_chain` when the chain is (re)built, so it is left zeroed.
/// `action_hash` is a *keyed* HMAC of the payload, so the attestation is
/// authentic — not a forgeable unkeyed hash.
fn provenance_entry(payload: &Session) -> WitnessEntry {
    WitnessEntry {
        prev_hash: [0u8; 32],
        action_hash: keyed_attestation(payload),
        timestamp_ns: now_ns(),
        witness_type: witness_type_provenance(),
    }
}

async fn persist(container: &RvfContainer, path: &str) -> anyhow::Result<()> {
    let bytes = serde_json::to_vec_pretty(container)?;
    if let Some(parent) = std::path::Path::new(path).parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp = format!("{}.tmp", path);
    tokio::fs::write(&tmp, &bytes).await?;
    tokio::fs::rename(&tmp, path).await?;
    Ok(())
}

/// Write a session to a genesis `.rvf` container (single-entry witness chain).
pub async fn write_session(session: &Session, path: &str) -> anyhow::Result<()> {
    write_container(session, &[provenance_entry(session)], path).await
}

/// Write a session with an explicit ordered list of prior witness entries plus
/// a fresh provenance entry appended for `session` (used when forking to extend
/// a parent's lineage). The chain is relinked so prev_hashes are consistent.
pub async fn write_container(
    session: &Session,
    prior_entries: &[WitnessEntry],
    path: &str,
) -> anyhow::Result<()> {
    let mut entries: Vec<WitnessEntry> = prior_entries.to_vec();
    // Ensure the final entry attests this exact payload.
    let attest = provenance_entry(session);
    match entries.last() {
        Some(last) if last.action_hash == attest.action_hash => {}
        _ => entries.push(attest),
    }
    let container = RvfContainer {
        version: "rvf-1".to_string(),
        payload: session.clone(),
        witness: hex::encode(create_witness_chain(&entries)),
    };
    persist(&container, path).await
}

/// Read and verify a container (chain integrity + payload attestation).
pub async fn read_container(path: &str) -> anyhow::Result<RvfContainer> {
    let bytes = tokio::fs::read(path).await?;
    let container: RvfContainer = serde_json::from_slice(&bytes)?;
    if !verify_container(&container) {
        anyhow::bail!("witness verification failed for {}", path);
    }
    Ok(container)
}

/// Read a verified session from an `.rvf` container.
pub async fn read_session(path: &str) -> anyhow::Result<Session> {
    Ok(read_container(path).await?.payload)
}

/// Decode the witness entries of a verified container (for lineage extension).
pub fn chain_entries(container: &RvfContainer) -> anyhow::Result<Vec<WitnessEntry>> {
    let chain = hex::decode(&container.witness)?;
    verify_witness_chain(&chain).map_err(|e| anyhow::anyhow!("invalid witness chain: {:?}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_then_read_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.rvf");
        let path_str = path.to_str().unwrap();

        let mut s = Session::new();
        s.name = "roundtrip".into();
        s.rvf_path = path_str.to_string();
        s.state.insert("hello".into(), "\"world\"".into());

        write_session(&s, path_str).await.unwrap();
        assert!(path.exists(), "the .rvf file must actually be created");

        let loaded = read_session(path_str).await.unwrap();
        assert_eq!(loaded, s, "loaded session must equal what was written");
    }

    #[tokio::test]
    async fn tampered_file_fails_to_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.rvf");
        let path_str = path.to_str().unwrap();

        let s = Session::new();
        write_session(&s, path_str).await.unwrap();

        let raw = tokio::fs::read_to_string(path_str).await.unwrap();
        let corrupted = raw.replace(&s.id.to_string(), &uuid::Uuid::new_v4().to_string());
        tokio::fs::write(path_str, corrupted).await.unwrap();

        assert!(
            read_session(path_str).await.is_err(),
            "reading a tampered container must error"
        );
    }

    #[tokio::test]
    async fn chain_grows_when_extended() {
        let dir = tempfile::tempdir().unwrap();
        let p1 = dir.path().join("a.rvf");
        let p1s = p1.to_str().unwrap();

        let mut parent = Session::new();
        parent.name = "p".into();
        write_session(&parent, p1s).await.unwrap();
        let parent_container = read_container(p1s).await.unwrap();
        let parent_entries = chain_entries(&parent_container).unwrap();
        assert_eq!(parent_entries.len(), 1, "genesis chain has one entry");

        // Extend: a child carrying the parent's lineage + its own entry.
        let mut child = Session::new();
        child.name = "c".into();
        let p2 = dir.path().join("b.rvf");
        let p2s = p2.to_str().unwrap();
        write_container(&child, &parent_entries, p2s).await.unwrap();

        let child_container = read_container(p2s).await.unwrap();
        let child_entries = chain_entries(&child_container).unwrap();
        assert_eq!(child_entries.len(), 2, "child chain extends the parent");
    }
}
