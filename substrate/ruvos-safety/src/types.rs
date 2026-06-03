use std::collections::HashMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Severity level of a safety concern or constraint enforcement tier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SafetyLevel {
    /// Enforcement is negligible; used for informational tagging only.
    Minimal,
    #[default]
    Low,
    Medium,
    High,
    /// System must halt or refuse the action when this level is triggered.
    Critical,
}

impl std::fmt::Display for SafetyLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SafetyLevel::Minimal => write!(f, "minimal"),
            SafetyLevel::Low => write!(f, "low"),
            SafetyLevel::Medium => write!(f, "medium"),
            SafetyLevel::High => write!(f, "high"),
            SafetyLevel::Critical => write!(f, "critical"),
        }
    }
}

/// Whether a constraint is fixed, soft-warned, or self-tuning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintType {
    /// Always enforced; violation is a hard refusal.
    Hard,
    /// Enforced but produces a warning rather than a refusal in isolation.
    Soft,
    /// Threshold adjusts over time based on observed outcomes.
    Adaptive,
}

/// A single named guardrail that can be evaluated against an observed value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConstraint {
    /// UUID v4 identifier.
    pub id: String,
    pub name: String,
    pub constraint_type: ConstraintType,
    /// Violation occurs when `current_value` exceeds this value.
    pub threshold: f64,
    /// Most recently observed measurement for this constraint.
    pub current_value: f64,
    /// Absolute floor for adaptive threshold changes.
    pub min_threshold: f64,
    /// Absolute ceiling for adaptive threshold changes.
    pub max_threshold: f64,
    /// Fraction by which an Adaptive threshold moves each adaptation cycle.
    pub adaptivity_rate: f64,
    /// Cumulative count of threshold violations recorded against this constraint.
    pub violation_count: u64,
    /// ISO-8601 creation timestamp.
    pub created_at: String,
    /// ISO-8601 timestamp of the last mutation.
    pub updated_at: String,
}

impl SafetyConstraint {
    /// Create a new constraint with generated UUID and current timestamps.
    pub fn new(
        name: impl Into<String>,
        constraint_type: ConstraintType,
        threshold: f64,
        min_threshold: f64,
        max_threshold: f64,
        adaptivity_rate: f64,
    ) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            constraint_type,
            threshold,
            current_value: 0.0,
            min_threshold,
            max_threshold,
            adaptivity_rate,
            violation_count: 0,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Returns `true` when the current value exceeds the threshold.
    pub fn is_violated(&self) -> bool {
        self.current_value > self.threshold
    }
}

/// A recorded breach of a `SafetyConstraint`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyViolation {
    /// UUID v4 identifier.
    pub id: String,
    pub constraint_id: String,
    pub constraint_name: String,
    pub level: SafetyLevel,
    pub observed_value: f64,
    pub threshold: f64,
    pub message: String,
    /// ISO-8601 timestamp of when the violation was detected.
    pub timestamp: String,
}

impl SafetyViolation {
    /// Construct a new violation record with generated UUID and current timestamp.
    pub fn new(
        constraint_id: impl Into<String>,
        constraint_name: impl Into<String>,
        level: SafetyLevel,
        observed_value: f64,
        threshold: f64,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            constraint_id: constraint_id.into(),
            constraint_name: constraint_name.into(),
            level,
            observed_value,
            threshold,
            message: message.into(),
            timestamp: Utc::now().to_rfc3339(),
        }
    }
}

/// Input to the safety validation pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRequest {
    /// Text content to inspect for dangerous patterns.
    pub content: String,
    /// Key/value context pairs (e.g. `"action" → "file_write"`).
    pub context: HashMap<String, String>,
    /// Minimum safety level the caller requires to be enforced.
    pub safety_level: SafetyLevel,
}

/// Result returned by `SafetyEngine::validate`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResponse {
    /// `true` when no Hard/Critical violations were detected.
    pub passed: bool,
    /// All violations detected during this validation pass.
    pub violations: Vec<SafetyViolation>,
    /// Aggregate safety health in `[0.0, 1.0]`; 1.0 means fully safe.
    pub safety_score: f64,
    /// Engine's recommendation for the appropriate safety level.
    pub recommended_level: SafetyLevel,
}
