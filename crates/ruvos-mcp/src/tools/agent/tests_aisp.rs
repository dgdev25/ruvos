//! Tests for ADR-034 phase 2: AISP prompt-precision layer wired into agent_spawn.
use super::handlers::AgentSpawnHandler;
use crate::paths;
use crate::tools::handler::ToolHandler;
use serde_json::json;

fn isolate() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    crate::paths::set_test_root(dir.path().to_path_buf());
    std::env::set_var("RUVOS_DISABLE_CLI_ROUTER", "1");
    dir
}

fn write_hooks_json(cfg: serde_json::Value) {
    let p = paths::data_root().join("hooks.json");
    std::fs::write(&p, cfg.to_string()).unwrap();
}

// ── disabled (default) ────────────────────────────────────────────────────────

#[tokio::test]
async fn aisp_absent_from_response_when_disabled() {
    let _g = isolate();
    let r = AgentSpawnHandler
        .execute(json!({"archetype": "coder", "prompt": "write a safe add function", "model": "m"}))
        .await
        .unwrap();
    assert_eq!(r["status"], "completed");
    assert!(r["aisp"].is_null(), "aisp must be absent when disabled");
}

#[tokio::test]
async fn aisp_disabled_artifact_has_no_lambda_block() {
    let _g = isolate();
    let r = AgentSpawnHandler
        .execute(json!({"archetype": "coder", "prompt": "write a safe add function", "model": "m"}))
        .await
        .unwrap();
    let content = std::fs::read_to_string(r["artifact_path"].as_str().unwrap()).unwrap();
    assert!(
        !content.contains("⟦Λ:Task⟧"),
        "lambda block must not appear when AISP is disabled"
    );
}

// ── enabled (warn_only — never blocks) ───────────────────────────────────────

#[tokio::test]
async fn aisp_assessment_present_when_enabled() {
    let _g = isolate();
    write_hooks_json(json!({"aisp": {"enabled": true, "warn_only": true, "auto_convert": true}}));
    let r = AgentSpawnHandler
        .execute(json!({"archetype": "coder", "prompt": "write a safe add function", "model": "m"}))
        .await
        .unwrap();
    assert_eq!(r["status"], "completed");
    let aisp = &r["aisp"];
    assert!(
        aisp.is_object(),
        "aisp assessment must be present when enabled"
    );
    assert!(aisp["tier"].is_string());
    assert!(aisp["delta"].is_number());
}

#[tokio::test]
async fn aisp_enabled_injects_lambda_block_into_artifact() {
    let _g = isolate();
    write_hooks_json(json!({"aisp": {"enabled": true, "warn_only": true, "auto_convert": true}}));
    let r = AgentSpawnHandler
        .execute(json!({"archetype": "coder", "prompt": "write a safe add function", "model": "m"}))
        .await
        .unwrap();
    let content = std::fs::read_to_string(r["artifact_path"].as_str().unwrap()).unwrap();
    assert!(
        content.contains("⟦Λ:Task⟧"),
        "lambda block must appear in artifact when AISP is enabled"
    );
}

#[tokio::test]
async fn aisp_artifact_still_contains_original_prose() {
    let _g = isolate();
    write_hooks_json(json!({"aisp": {"enabled": true, "warn_only": true, "auto_convert": true}}));
    let r = AgentSpawnHandler
        .execute(json!({"archetype": "coder", "prompt": "write a safe add function", "model": "m"}))
        .await
        .unwrap();
    let content = std::fs::read_to_string(r["artifact_path"].as_str().unwrap()).unwrap();
    assert!(
        content.contains("write a safe add function"),
        "original prose must survive the AISP injection"
    );
}

#[tokio::test]
async fn aisp_warn_only_never_blocks_spawn() {
    let _g = isolate();
    write_hooks_json(json!({
        "aisp": {"enabled": true, "warn_only": true, "auto_convert": true, "min_tier": "platinum"}
    }));
    let r = AgentSpawnHandler
        .execute(json!({"archetype": "coder", "prompt": "do the thing somehow", "model": "m"}))
        .await
        .unwrap();
    assert_eq!(r["status"], "completed", "warn_only must never block");
    assert!(r["aisp"].is_object());
}

// ── hard quality gate ─────────────────────────────────────────────────────────

#[tokio::test]
async fn aisp_hard_gate_blocks_vague_prose() {
    let _g = isolate();
    write_hooks_json(json!({
        "aisp": {"enabled": true, "warn_only": false, "auto_convert": true, "min_tier": "platinum"}
    }));
    let r = AgentSpawnHandler
        .execute(json!({"archetype": "coder", "prompt": "do the thing somehow", "model": "m"}))
        .await
        .unwrap();
    assert_eq!(r["status"], "blocked");
    assert_eq!(r["reason"], "aisp_quality_gate");
    assert!(
        r["aisp"].is_object(),
        "blocked response must include assessment"
    );
    assert!(r["aisp"]["tier"].is_string());
}

#[tokio::test]
async fn aisp_hard_gate_blocked_response_has_no_agent_id() {
    let _g = isolate();
    write_hooks_json(json!({
        "aisp": {"enabled": true, "warn_only": false, "auto_convert": true, "min_tier": "platinum"}
    }));
    let r = AgentSpawnHandler
        .execute(json!({"archetype": "coder", "prompt": "do the thing somehow", "model": "m"}))
        .await
        .unwrap();
    assert_eq!(r["status"], "blocked");
    assert!(r["agent_id"].is_null(), "no agent spawned when blocked");
}
