//! .rvf container write/read operations.

use crate::Session;

/// Write a session to .rvf container.
pub async fn write_session(_session: &Session, _path: &str) -> anyhow::Result<()> {
    // TODO: Serialize session to .rvf format
    // - Write metadata (id, timestamps)
    // - Write memory snapshot (serde_json)
    // - Write signature chain (rvf-crypto)
    Ok(())
}

/// Read a session from .rvf container.
pub async fn read_session(_path: &str) -> anyhow::Result<Session> {
    // TODO: Deserialize session from .rvf format
    // - Parse metadata
    // - Restore memory snapshot
    // - Verify signature chain
    Ok(Session::new())
}
