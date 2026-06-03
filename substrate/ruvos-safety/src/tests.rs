use std::collections::HashMap;

use tempfile::TempDir;

use crate::{
    engine::SafetyEngine,
    types::{SafetyLevel, ValidationRequest},
};

fn fresh_engine() -> (SafetyEngine, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let engine = SafetyEngine::new(dir.path().to_str().unwrap());
    (engine, dir)
}

// 1. Default constraints are created on first run.
#[test]
fn default_constraints_are_added() {
    let (engine, _dir) = fresh_engine();
    assert_eq!(engine.constraints().len(), 5);
    let names: Vec<&str> = engine
        .constraints()
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    assert!(names.contains(&"file_access_rate"));
    assert!(names.contains(&"network_request_rate"));
    assert!(names.contains(&"agent_spawn_rate"));
    assert!(names.contains(&"memory_usage_ratio"));
    assert!(names.contains(&"error_rate"));
}

// 2. Safe content with no context → passes, no violations.
#[test]
fn validate_safe_content_passes() {
    let (engine, _dir) = fresh_engine();
    let req = ValidationRequest {
        content: "Hello, world!".to_string(),
        context: HashMap::new(),
        safety_level: SafetyLevel::Low,
    };
    let resp = engine.validate(&req);
    assert!(resp.passed);
    assert!(resp.violations.is_empty());
    assert!((resp.safety_score - 1.0).abs() < f64::EPSILON);
}

// 3. Content containing a dangerous shell pattern → High violation.
#[test]
fn validate_dangerous_content_flagged() {
    let (engine, _dir) = fresh_engine();
    let req = ValidationRequest {
        content: "Please run rm -rf /tmp/test".to_string(),
        context: HashMap::new(),
        safety_level: SafetyLevel::Low,
    };
    let resp = engine.validate(&req);
    assert!(!resp.passed);
    assert!(!resp.violations.is_empty());
    assert!(resp.violations.iter().any(|v| v.level == SafetyLevel::High));
}

// 4. record_outcome with observed > threshold logs a violation.
#[test]
fn record_outcome_triggers_violation() {
    let (mut engine, _dir) = fresh_engine();
    let id = engine
        .constraints()
        .iter()
        .find(|c| c.name == "file_access_rate")
        .map(|c| c.id.clone())
        .unwrap();

    engine.record_outcome(&id, 200.0);

    let violated: Vec<_> = engine
        .violations_log()
        .iter()
        .filter(|v| v.constraint_id == id)
        .collect();
    assert!(!violated.is_empty(), "expected at least one violation");
    assert_eq!(violated[0].observed_value, 200.0);
}

// 5. adapt_boundaries tightens a Hard constraint that has violations.
#[test]
fn adapt_boundaries_tightens_violated_constraint() {
    let (mut engine, _dir) = fresh_engine();
    let id = engine
        .constraints()
        .iter()
        .find(|c| c.name == "file_access_rate")
        .map(|c| c.id.clone())
        .unwrap();

    let threshold_before = engine
        .constraints()
        .iter()
        .find(|c| c.id == id)
        .map(|c| c.threshold)
        .unwrap();

    // Record a violation so violation_count > 0.
    engine.record_outcome(&id, 200.0);
    engine.adapt_boundaries();

    let threshold_after = engine
        .constraints()
        .iter()
        .find(|c| c.id == id)
        .map(|c| c.threshold)
        .unwrap();

    assert!(
        threshold_after < threshold_before,
        "expected threshold to decrease after violations; \
         before={threshold_before}, after={threshold_after}"
    );
}

// 6. Save + reload gives the same constraint count and violation log.
#[test]
fn persist_roundtrip() {
    let dir = TempDir::new().expect("tempdir");
    let data_path = dir.path().to_str().unwrap();

    let mut engine = SafetyEngine::new(data_path);
    let id = engine
        .constraints()
        .iter()
        .find(|c| c.name == "file_access_rate")
        .map(|c| c.id.clone())
        .unwrap();
    engine.record_outcome(&id, 200.0);
    engine.save().expect("save");

    let engine2 = SafetyEngine::new(data_path);
    assert_eq!(engine.constraints().len(), engine2.constraints().len());
    assert_eq!(
        engine.violations_log().len(),
        engine2.violations_log().len(),
        "violations_log length mismatch after reload"
    );
}
