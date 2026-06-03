//! `ruvos-safety` — Behavioral safety guardrails and adaptive boundaries for
//! rUvOS agents.
//!
//! Inspired by SAFLA's `safety_validation.py` and
//! `adaptive_safety_boundaries.py`, rewritten natively in Rust with no Python
//! dependency.
//!
//! # Quick start
//!
//! ```rust
//! use ruvos_safety::{SafetyEngine, ValidationRequest, SafetyLevel};
//! use std::collections::HashMap;
//!
//! let mut engine = SafetyEngine::new("/tmp/ruvos-safety-example");
//!
//! let req = ValidationRequest {
//!     content: "SELECT * FROM users".to_string(),
//!     context: HashMap::new(),
//!     safety_level: SafetyLevel::Low,
//! };
//!
//! let resp = engine.validate(&req);
//! assert!(resp.passed);
//! assert!((resp.safety_score - 1.0).abs() < f64::EPSILON);
//! ```

pub mod engine;
pub mod persist;
pub mod types;

pub use engine::SafetyEngine;
pub use types::{
    ConstraintType, SafetyConstraint, SafetyLevel, SafetyViolation, ValidationRequest,
    ValidationResponse,
};
