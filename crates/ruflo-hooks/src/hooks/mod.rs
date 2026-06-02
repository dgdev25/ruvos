//! Hook type definitions.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Hook kind discriminator.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum HookKind {
    #[serde(rename = "task")]
    Task,
    #[serde(rename = "edit")]
    Edit,
    #[serde(rename = "command")]
    Command,
    #[serde(rename = "session")]
    Session,
}

/// Unified hook payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookPayload {
    pub kind: HookKind,
    pub data: Value,
}

impl HookPayload {
    pub fn new(kind: HookKind, data: Value) -> Self {
        Self { kind, data }
    }
}
