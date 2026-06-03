//! Record types for the `ruvos-store` redb-backed store.
//!
//! These port the capability of `ruv-swarm-persistence`'s SQLite models into
//! plain serde structs. Enums are simplified to `String` where that is cleaner
//! for a JSON-on-redb store (status / type fields). Timestamps are stored as
//! RFC3339 strings (for display) plus integer UNIX-second fields where range
//! queries / ordering matter.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Current UNIX timestamp in whole seconds.
pub(crate) fn now_secs() -> i64 {
    Utc::now().timestamp()
}

/// An agent participating in a swarm.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRecord {
    pub id: String,
    pub name: String,
    pub agent_type: String,
    /// Free-form status string (e.g. "initializing", "active", "idle",
    /// "busy", "paused", "error", "shutdown").
    pub status: String,
    pub capabilities: Vec<String>,
    pub metadata: HashMap<String, serde_json::Value>,
    /// Last heartbeat (UNIX seconds).
    pub heartbeat: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl AgentRecord {
    /// Construct a new agent with a generated id and "initializing" status.
    pub fn new(name: impl Into<String>, agent_type: impl Into<String>) -> Self {
        let now = now_secs();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            agent_type: agent_type.into(),
            status: "initializing".to_string(),
            capabilities: Vec::new(),
            metadata: HashMap::new(),
            heartbeat: now,
            created_at: now,
            updated_at: now,
        }
    }
}

/// A unit of work in the swarm.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TaskRecord {
    pub id: String,
    pub task_type: String,
    /// Priority where higher = more urgent (0=low .. 3=critical).
    pub priority: i32,
    /// Free-form status (e.g. "pending", "assigned", "running",
    /// "completed", "failed", "cancelled").
    pub status: String,
    pub assigned_to: Option<String>,
    pub payload: serde_json::Value,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub retry_count: u32,
    pub max_retries: u32,
    pub dependencies: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

impl TaskRecord {
    /// Construct a new pending task with a generated id.
    pub fn new(task_type: impl Into<String>, payload: serde_json::Value, priority: i32) -> Self {
        let now = now_secs();
        Self {
            id: Uuid::new_v4().to_string(),
            task_type: task_type.into(),
            priority,
            status: "pending".to_string(),
            assigned_to: None,
            payload,
            result: None,
            error: None,
            retry_count: 0,
            max_retries: 3,
            dependencies: Vec::new(),
            created_at: now,
            updated_at: now,
            started_at: None,
            completed_at: None,
        }
    }
}

/// An audit-log event (event sourcing).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EventRecord {
    pub id: String,
    pub event_type: String,
    pub agent_id: Option<String>,
    pub task_id: Option<String>,
    pub payload: serde_json::Value,
    pub metadata: HashMap<String, serde_json::Value>,
    /// Event time (UNIX seconds) — used by `events_since` range queries.
    pub timestamp: i64,
    pub sequence: u64,
}

impl EventRecord {
    /// Construct a new event with a generated id, stamped now.
    pub fn new(event_type: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            event_type: event_type.into(),
            agent_id: None,
            task_id: None,
            payload,
            metadata: HashMap::new(),
            timestamp: now_secs(),
            sequence: 0,
        }
    }
}

/// An inter-agent message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MessageRecord {
    pub id: String,
    pub from_agent: String,
    pub to_agent: String,
    pub message_type: String,
    pub content: serde_json::Value,
    /// Free-form priority (e.g. "low", "normal", "high", "urgent").
    pub priority: String,
    pub read: bool,
    pub created_at: i64,
    pub read_at: Option<i64>,
}

impl MessageRecord {
    /// Construct a new unread message with a generated id.
    pub fn new(
        from_agent: impl Into<String>,
        to_agent: impl Into<String>,
        message_type: impl Into<String>,
        content: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            from_agent: from_agent.into(),
            to_agent: to_agent.into(),
            message_type: message_type.into(),
            content,
            priority: "normal".to_string(),
            read: false,
            created_at: now_secs(),
            read_at: None,
        }
    }
}

/// A performance metric data point.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricRecord {
    pub id: String,
    pub metric_type: String,
    pub agent_id: Option<String>,
    pub value: f64,
    pub unit: String,
    pub tags: HashMap<String, String>,
    /// Sample time (UNIX seconds) — used by `aggregated_metric` windows.
    pub timestamp: i64,
}

impl MetricRecord {
    /// Construct a new metric data point with a generated id, stamped now.
    pub fn new(metric_type: impl Into<String>, value: f64, unit: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            metric_type: metric_type.into(),
            agent_id: None,
            value,
            unit: unit.into(),
            tags: HashMap::new(),
            timestamp: now_secs(),
        }
    }
}

/// The complete serializable snapshot of every record in the store.
/// Used as the `.rvf` snapshot payload.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct StoreSnapshot {
    pub agents: Vec<AgentRecord>,
    pub tasks: Vec<TaskRecord>,
    pub events: Vec<EventRecord>,
    pub messages: Vec<MessageRecord>,
    pub metrics: Vec<MetricRecord>,
}
