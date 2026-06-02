//! COW-branch session forking.

use crate::Session;
use uuid::Uuid;

/// Fork a session for parallel exploration using COW semantics.
pub async fn fork_session(_source_id: Uuid) -> anyhow::Result<Session> {
    // TODO: Use rvf-cow to fork session
    // - Create new session with forked id
    // - Link to parent via COW mechanism
    // - Isolate memory mutations
    Ok(Session {
        id: Uuid::new_v4(),
        rvf_path: String::new(),
    })
}
