use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum HookKind {
    Task,
    Edit,
    Command,
    Session,
}

impl HookKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookKind::Task => "task",
            HookKind::Edit => "edit",
            HookKind::Command => "command",
            HookKind::Session => "session",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookEvent {
    pub id: String,
    pub kind: HookKind,
    pub phase: HookPhase,
    pub timestamp: DateTime<Utc>,
    pub payload: serde_json::Value,
    pub status: EventStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum HookPhase {
    Pre,
    Post,
}

impl HookPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookPhase::Pre => "pre",
            HookPhase::Post => "post",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EventStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreHookRequest {
    pub kind: HookKind,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostHookRequest {
    pub kind: HookKind,
    pub payload: serde_json::Value,
    pub outcome: HookOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookOutcome {
    pub success: bool,
    pub message: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookResponse {
    pub status: String,
    pub routing: Option<HookRouting>,
    pub context: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookRouting {
    pub model: Option<String>,
    pub archetype: Option<String>,
}
