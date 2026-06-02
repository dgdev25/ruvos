//! Session domain tools (3): create, resume, fork

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: SessionId,
    pub rvf_path: String,
    pub created_at: String,
}

/// Start a session, return id, persist as .rvf.
pub async fn create() -> anyhow::Result<SessionId> {
    let id = SessionId(Uuid::new_v4());
    // TODO: Initialize .rvf container and write metadata
    Ok(id)
}

/// Restore a session by id (full context + memory).
pub async fn resume(_id: &SessionId) -> anyhow::Result<SessionMetadata> {
    // TODO: Read from .rvf container, restore memory
    Ok(SessionMetadata {
        id: SessionId(Uuid::new_v4()),
        rvf_path: String::new(),
        created_at: String::new(),
    })
}

/// COW-branch a session for parallel exploration.
pub async fn fork(_source_id: &SessionId) -> anyhow::Result<SessionId> {
    // TODO: Use rvf-cow to fork session
    let forked_id = SessionId(Uuid::new_v4());
    Ok(forked_id)
}
