//! Core node types: `EntityNode` and `Episode`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A named entity extracted from one or more episodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EntityNode {
    /// Stable surrogate key.
    pub id: Uuid,
    /// Canonical name (case-folded when deduplicating).
    pub name: String,
    /// Running summary built up across episodes.
    pub summary: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EntityNode {
    /// Create a brand-new entity with an empty summary.
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            summary: String::new(),
            created_at: now,
            updated_at: now,
        }
    }
}

/// The raw input event that produced entities and edges.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Episode {
    pub id: Uuid,
    pub content: String,
    pub source: String,
    pub created_at: DateTime<Utc>,
}

impl Episode {
    pub fn new(content: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            content: content.into(),
            source: source.into(),
            created_at: Utc::now(),
        }
    }
}
