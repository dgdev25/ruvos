//! ruvos-session: .rvf container write/read, fork (COW-branch), signature verification.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod fork;
pub mod rvf;
pub mod verify;

pub use fork::fork_session;
pub use rvf::{read_session, write_session};
pub use verify::verify_signature;

/// Session metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub rvf_path: String,
}

impl Session {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            rvf_path: String::new(),
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}
