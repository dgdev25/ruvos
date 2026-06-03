//! `EntityEdge` — a directed, temporally-bounded relationship between two entities.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A directed relationship between two entities extracted from an episode.
///
/// Temporal semantics:
/// - `valid_at = None` means "always true since creation" (open start).
/// - `invalid_at = None` means "still current" (open end).
/// - Setting `invalid_at` marks the fact as no longer true from that point.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EntityEdge {
    pub id: Uuid,
    /// Source entity node id.
    pub source_id: Uuid,
    /// Target entity node id.
    pub target_id: Uuid,
    /// Relationship type label (e.g. "mentions", "co-occurs", "relates_to").
    pub name: String,
    /// Human-readable statement of the fact (e.g. "Alice works at Acme Corp").
    pub fact: String,
    /// Optional explicit validity start.
    pub valid_at: Option<DateTime<Utc>>,
    /// When set, the fact is no longer considered current after this time.
    pub invalid_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl EntityEdge {
    pub fn new(
        source_id: Uuid,
        target_id: Uuid,
        name: impl Into<String>,
        fact: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            source_id,
            target_id,
            name: name.into(),
            fact: fact.into(),
            valid_at: None,
            invalid_at: None,
            created_at: Utc::now(),
        }
    }

    /// Return true if this edge is "active" at the given instant:
    /// - created before (or at) `at`
    /// - not yet invalidated (or invalidated after `at`)
    pub fn is_active_at(&self, at: DateTime<Utc>) -> bool {
        self.created_at <= at && self.invalid_at.map_or(true, |inv| inv > at)
    }
}
