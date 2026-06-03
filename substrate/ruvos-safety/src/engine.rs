use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    persist::{atomic_write, load_json},
    types::{
        ConstraintType, SafetyConstraint, SafetyLevel, SafetyViolation, ValidationRequest,
        ValidationResponse,
    },
};

// ---------------------------------------------------------------------------
// Serialisable snapshot (what we persist to disk)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    constraints: Vec<SafetyConstraint>,
    violations_log: Vec<SafetyViolation>,
}

// ---------------------------------------------------------------------------
// Content-pattern rules embedded in the engine
// ---------------------------------------------------------------------------

/// Static dangerous patterns that trigger a High-level content violation.
static DANGEROUS_PATTERNS: &[&str] = &["rm -rf", "DROP TABLE", "DELETE FROM"];

// ---------------------------------------------------------------------------
// SafetyEngine
// ---------------------------------------------------------------------------

/// Behavioral safety guardrail engine.
///
/// Holds a set of `SafetyConstraint` records (persisted to disk) and validates
/// `ValidationRequest` payloads against them.  Adaptive constraints self-tune
/// their thresholds based on observed outcomes.
pub struct SafetyEngine {
    constraints: Vec<SafetyConstraint>,
    violations_log: Vec<SafetyViolation>,
    data_path: PathBuf,
}

impl SafetyEngine {
    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    /// Load persisted state from `data_path/safety.json`, or start fresh with
    /// the five built-in default constraints when no saved state exists.
    pub fn new(data_path: &str) -> Self {
        let path = PathBuf::from(data_path).join("safety.json");
        let snapshot: Option<Snapshot> = load_json(&path).unwrap_or(None);

        let (constraints, violations_log) = match snapshot {
            Some(s) => (s.constraints, s.violations_log),
            None => (Self::default_constraints(), Vec::new()),
        };

        Self {
            constraints,
            violations_log,
            data_path: PathBuf::from(data_path),
        }
    }

    /// The five SAFLA-inspired default constraints.
    fn default_constraints() -> Vec<SafetyConstraint> {
        vec![
            SafetyConstraint::new(
                "file_access_rate",
                ConstraintType::Hard,
                100.0,
                10.0,
                500.0,
                0.0,
            ),
            SafetyConstraint::new(
                "network_request_rate",
                ConstraintType::Hard,
                50.0,
                5.0,
                200.0,
                0.0,
            ),
            SafetyConstraint::new(
                "agent_spawn_rate",
                ConstraintType::Adaptive,
                10.0,
                1.0,
                50.0,
                0.05,
            ),
            SafetyConstraint::new(
                "memory_usage_ratio",
                ConstraintType::Soft,
                0.85,
                0.5,
                1.0,
                0.0,
            ),
            SafetyConstraint::new("error_rate", ConstraintType::Adaptive, 0.1, 0.01, 0.5, 0.02),
        ]
    }

    // -----------------------------------------------------------------------
    // Constraint management
    // -----------------------------------------------------------------------

    /// Append a new constraint.  Returns an error if a constraint with the
    /// same `id` already exists.
    pub fn add_constraint(&mut self, constraint: SafetyConstraint) -> Result<()> {
        if self.constraints.iter().any(|c| c.id == constraint.id) {
            anyhow::bail!("constraint with id '{}' already exists", constraint.id);
        }
        self.constraints.push(constraint);
        Ok(())
    }

    /// Remove the constraint with the given `id`.  Returns an error when not
    /// found.
    pub fn remove_constraint(&mut self, id: &str) -> Result<()> {
        let pos = self
            .constraints
            .iter()
            .position(|c| c.id == id)
            .with_context(|| format!("constraint '{}' not found", id))?;
        self.constraints.remove(pos);
        Ok(())
    }

    /// Read-only slice of active constraints.
    pub fn constraints(&self) -> &[SafetyConstraint] {
        &self.constraints
    }

    /// Read-only slice of the recorded violation log.
    pub fn violations_log(&self) -> &[SafetyViolation] {
        &self.violations_log
    }

    // -----------------------------------------------------------------------
    // Validation
    // -----------------------------------------------------------------------

    /// Validate `request` against every active constraint.
    ///
    /// Evaluation order:
    /// 1. Context-keyed rate constraints (file, network, agent_spawn).
    /// 2. Content pattern scan (dangerous shell / SQL patterns → High).
    /// 3. `safety_score` = 1.0 - violations / (constraints + 1).
    /// 4. `recommended_level` derived from the worst violation seen.
    pub fn validate(&self, request: &ValidationRequest) -> ValidationResponse {
        let mut violations: Vec<SafetyViolation> = Vec::new();

        // --- Context-based constraint checks --------------------------------
        if let Some(action) = request.context.get("action") {
            match action.as_str() {
                "file_write" | "file_read" => {
                    self.check_named_constraint(
                        "file_access_rate",
                        &request.safety_level,
                        &mut violations,
                    );
                }
                "network" => {
                    self.check_named_constraint(
                        "network_request_rate",
                        &request.safety_level,
                        &mut violations,
                    );
                }
                "agent_spawn" => {
                    self.check_named_constraint(
                        "agent_spawn_rate",
                        &request.safety_level,
                        &mut violations,
                    );
                }
                _ => {}
            }
        }

        // --- Content pattern scan -------------------------------------------
        let lower = request.content.to_lowercase();
        for pattern in DANGEROUS_PATTERNS {
            if lower.contains(&pattern.to_lowercase()) {
                violations.push(SafetyViolation::new(
                    "content_scan",
                    "content_pattern",
                    SafetyLevel::High,
                    1.0,
                    0.0,
                    format!("Dangerous pattern detected in content: '{pattern}'"),
                ));
            }
        }

        // --- Aggregate score & recommended level ----------------------------
        let safety_score = self.compute_score(violations.len());
        let recommended_level = self.recommend_level(&violations);
        let passed = violations.iter().all(|v| v.level < SafetyLevel::High);

        ValidationResponse {
            passed,
            violations,
            safety_score,
            recommended_level,
        }
    }

    // -----------------------------------------------------------------------
    // Outcome recording
    // -----------------------------------------------------------------------

    /// Update `current_value` for the named constraint, adapt Adaptive
    /// thresholds, and record a `SafetyViolation` when the new value exceeds
    /// the threshold.
    pub fn record_outcome(&mut self, constraint_id: &str, observed: f64) {
        let now = Utc::now().to_rfc3339();

        if let Some(c) = self.constraints.iter_mut().find(|c| c.id == constraint_id) {
            c.current_value = observed;
            c.updated_at = now.clone();

            if c.is_violated() {
                c.violation_count += 1;

                // Tighten Adaptive thresholds on violation.
                if c.constraint_type == ConstraintType::Adaptive {
                    let delta = c.threshold * c.adaptivity_rate;
                    c.threshold = (c.threshold - delta).max(c.min_threshold);
                }

                let level = match c.constraint_type {
                    ConstraintType::Hard => SafetyLevel::Critical,
                    ConstraintType::Soft => SafetyLevel::Medium,
                    ConstraintType::Adaptive => SafetyLevel::High,
                };

                let v = SafetyViolation::new(
                    c.id.clone(),
                    c.name.clone(),
                    level,
                    observed,
                    c.threshold,
                    format!(
                        "Constraint '{}' violated: observed {:.3} > threshold {:.3}",
                        c.name, observed, c.threshold,
                    ),
                );
                self.violations_log.push(v);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Boundary adaptation
    // -----------------------------------------------------------------------

    /// Global adaptation sweep:
    /// - **Hard** constraints that have accrued any violations have their
    ///   threshold tightened by 5 % (floor: `min_threshold`).
    /// - **Adaptive** constraints with no violations in the last 24 h have
    ///   their threshold relaxed by `adaptivity_rate` (ceiling:
    ///   `max_threshold`).
    pub fn adapt_boundaries(&mut self) {
        let now = Utc::now();
        let cutoff = now - chrono::Duration::hours(24);

        for c in &mut self.constraints {
            match c.constraint_type {
                ConstraintType::Hard if c.violation_count > 0 => {
                    let delta = c.threshold * 0.05;
                    c.threshold = (c.threshold - delta).max(c.min_threshold);
                    c.updated_at = now.to_rfc3339();
                }
                ConstraintType::Adaptive => {
                    // Check whether any violation was recorded in the last 24 h.
                    let recent = self.violations_log.iter().any(|v| {
                        v.constraint_id == c.id
                            && chrono::DateTime::parse_from_rfc3339(&v.timestamp)
                                .map(|t| t.with_timezone(&Utc) > cutoff)
                                .unwrap_or(false)
                    });

                    if !recent {
                        let delta = c.threshold * c.adaptivity_rate;
                        c.threshold = (c.threshold + delta).min(c.max_threshold);
                        c.updated_at = now.to_rfc3339();
                    }
                }
                _ => {}
            }
        }
    }

    // -----------------------------------------------------------------------
    // Aggregate health
    // -----------------------------------------------------------------------

    /// Overall safety health in `[0.0, 1.0]`.
    ///
    /// Computed as `1.0 - (total_violations / (constraints * 10 + 1))`,
    /// clamped to `[0.0, 1.0]`.  A fresh engine with zero violations returns
    /// `1.0`.
    pub fn safety_score(&self) -> f64 {
        let total_violations: u64 = self.constraints.iter().map(|c| c.violation_count).sum();
        let denominator = (self.constraints.len() as u64 * 10 + 1) as f64;
        let raw = 1.0 - (total_violations as f64 / denominator);
        raw.clamp(0.0, 1.0)
    }

    // -----------------------------------------------------------------------
    // Violation queries
    // -----------------------------------------------------------------------

    /// Return all violations whose `timestamp` is >= `timestamp` (RFC-3339).
    /// Malformed timestamp strings are silently skipped.
    pub fn violations_since(&self, timestamp: &str) -> Vec<SafetyViolation> {
        let cutoff = match chrono::DateTime::parse_from_rfc3339(timestamp) {
            Ok(t) => t.with_timezone(&Utc),
            Err(_) => return self.violations_log.clone(),
        };

        self.violations_log
            .iter()
            .filter(|v| {
                chrono::DateTime::parse_from_rfc3339(&v.timestamp)
                    .map(|t| t.with_timezone(&Utc) >= cutoff)
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    // -----------------------------------------------------------------------
    // Persistence
    // -----------------------------------------------------------------------

    /// Atomically persist the engine state to `<data_path>/safety.json`.
    pub fn save(&self) -> Result<()> {
        let path = self.data_path.join("safety.json");
        let snapshot = Snapshot {
            constraints: self.constraints.clone(),
            violations_log: self.violations_log.clone(),
        };
        atomic_write(&path, &snapshot).context("save safety engine state")
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Find the constraint by name and push a violation if its current value
    /// exceeds its threshold.
    fn check_named_constraint(
        &self,
        name: &str,
        _min_level: &SafetyLevel,
        violations: &mut Vec<SafetyViolation>,
    ) {
        if let Some(c) = self.constraints.iter().find(|c| c.name == name) {
            if c.is_violated() {
                let level = match c.constraint_type {
                    ConstraintType::Hard => SafetyLevel::Critical,
                    ConstraintType::Soft => SafetyLevel::Medium,
                    ConstraintType::Adaptive => SafetyLevel::High,
                };
                violations.push(SafetyViolation::new(
                    c.id.clone(),
                    c.name.clone(),
                    level,
                    c.current_value,
                    c.threshold,
                    format!(
                        "Rate constraint '{}' exceeded: {:.1} > {:.1}",
                        c.name, c.current_value, c.threshold,
                    ),
                ));
            }
        }
    }

    /// `1.0 - violation_count / (constraints + 1)` clamped to `[0.0, 1.0]`.
    fn compute_score(&self, violation_count: usize) -> f64 {
        let denom = (self.constraints.len() + 1) as f64;
        (1.0 - violation_count as f64 / denom).clamp(0.0, 1.0)
    }

    /// Derive the recommended level from the worst violation.
    fn recommend_level(&self, violations: &[SafetyViolation]) -> SafetyLevel {
        violations
            .iter()
            .map(|v| v.level.clone())
            .max()
            .unwrap_or(SafetyLevel::Minimal)
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
