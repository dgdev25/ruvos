//! Tests for the `[<template> orchestration as <archetype>]` dispatch path.
use super::artifact::{
    build_artifact, build_orch_artifact, extract_structured_output, parse_orch_prompt,
};
use super::handlers::{AgentMessageHandler, AgentSpawnHandler, AgentStatusHandler};
use crate::tools::gov::GovEventsHandler;
use crate::tools::handler::ToolHandler;
use serde_json::json;

fn isolate() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    crate::paths::set_test_root(dir.path().to_path_buf());
    // Hermetic: disable CliRouter auto-detection so run_task uses the
    // deterministic placeholder artifact (see agent/mod.rs isolate()).
    std::env::set_var("RUVOS_DISABLE_CLI_ROUTER", "1");
    dir
}

async fn spawn_orch(archetype: &str, prompt: &str) -> serde_json::Value {
    AgentSpawnHandler
        .execute(json!({"archetype": archetype, "prompt": prompt, "model": "claude-haiku-4-5"}))
        .await
        .unwrap()
}

// ── parse_orch_prompt unit tests ──────────────────────────────────────────────

#[test]
fn parse_happy_path_feature_coder() {
    assert_eq!(
        parse_orch_prompt("coder", "[feature orchestration as coder] add rate limiter"),
        Some(("feature", "add rate limiter"))
    );
}

#[test]
fn parse_bugfix_template() {
    assert_eq!(
        parse_orch_prompt("coder", "[bugfix orchestration as coder] fix null deref"),
        Some(("bugfix", "fix null deref"))
    );
}

#[test]
fn parse_archetype_mismatch_returns_none() {
    assert_eq!(
        parse_orch_prompt("coder", "[feature orchestration as planner] add auth"),
        None
    );
}

#[test]
fn parse_plain_prompt_returns_none() {
    assert_eq!(
        parse_orch_prompt("coder", "implement a POST /users endpoint"),
        None
    );
}

#[test]
fn parse_empty_task_after_bracket_returns_none() {
    assert_eq!(
        parse_orch_prompt("coder", "[feature orchestration as coder] "),
        None
    );
}

#[test]
fn parse_unclosed_bracket_returns_none() {
    assert_eq!(
        parse_orch_prompt("coder", "[feature orchestration as coder add auth"),
        None
    );
}

#[test]
fn parse_no_space_after_bracket_returns_none() {
    assert_eq!(
        parse_orch_prompt("coder", "[feature orchestration as coder]no space"),
        None
    );
}

#[test]
fn parse_multiline_prompt_takes_first_line_only() {
    assert_eq!(
        parse_orch_prompt(
            "coder",
            "[feature orchestration as coder] add auth\nextra context"
        ),
        Some(("feature", "add auth"))
    );
}

#[test]
fn parse_refactor_template() {
    assert_eq!(
        parse_orch_prompt(
            "coder",
            "[refactor orchestration as coder] extract service layer"
        ),
        Some(("refactor", "extract service layer"))
    );
}

#[test]
fn parse_security_template() {
    assert_eq!(
        parse_orch_prompt(
            "coder",
            "[security orchestration as coder] harden auth middleware"
        ),
        Some(("security", "harden auth middleware"))
    );
}

// ── build_orch_artifact unit tests ────────────────────────────────────────────

fn coder_orch_artifact() -> String {
    let p = "[feature orchestration as coder] add rate limiter";
    build_orch_artifact("coder", "feature", "add rate limiter", p)
}

#[test]
fn orch_artifact_header_uses_archetype_label() {
    assert!(coder_orch_artifact().starts_with("# coder agent"));
}

#[test]
fn orch_artifact_template_section() {
    assert!(coder_orch_artifact().contains("## Template\nfeature"));
}

#[test]
fn orch_artifact_task_section() {
    assert!(coder_orch_artifact().contains("## Task\nadd rate limiter"));
}

#[test]
fn orch_artifact_plan_consumed_section_present() {
    assert!(coder_orch_artifact().contains("## Plan Consumed"));
}

#[test]
fn orch_artifact_implementation_section_present() {
    assert!(coder_orch_artifact().contains("## Implementation"));
}

#[test]
fn orch_artifact_no_prior_context_shows_placeholder() {
    let a = coder_orch_artifact();
    let idx = a.find("## Plan Consumed").unwrap();
    assert!(a[idx..].contains("(no prior plan)"));
}

#[test]
fn orch_artifact_prior_plan_injected() {
    let sep = "\n\nPrevious artifact to consume:\n";
    let full = format!(
        "[feature orchestration as coder] add auth{sep}# planner agent\n## Ordered Delivery Steps\n- coder: implement it\n"
    );
    let a = build_orch_artifact("coder", "feature", "add auth", &full);
    let idx = a.find("## Plan Consumed").unwrap();
    let section = &a[idx..];
    assert!(section.contains("planner agent"));
    assert!(section.contains("coder: implement it"));
}

#[test]
fn orch_artifact_implementation_has_four_steps() {
    let a = coder_orch_artifact();
    let idx = a.find("## Implementation").unwrap();
    let imp = &a[idx..];
    assert!(imp.contains("1."));
    assert!(imp.contains("2."));
    assert!(imp.contains("3."));
    assert!(imp.contains("4."));
}

#[test]
fn orch_artifact_step4_mentions_cargo_check() {
    let a = coder_orch_artifact();
    let idx = a.find("## Implementation").unwrap();
    assert!(a[idx..].contains("cargo check"));
}

#[test]
fn orch_artifact_double_separator_captures_from_first() {
    let sep = "\n\nPrevious artifact to consume:\n";
    let full = format!("[feature orchestration as coder] x{sep}first{sep}second");
    let a = build_orch_artifact("coder", "feature", "x", &full);
    let idx = a.find("## Plan Consumed").unwrap();
    let section = &a[idx..];
    assert!(
        section.contains("first"),
        "must include text after the first separator"
    );
}

// ── build_artifact dispatch tests ────────────────────────────────────────────

#[test]
fn orch_prompt_bypasses_post_users_specialisation() {
    let a = build_artifact(
        "coder",
        "[feature orchestration as coder] build POST /users",
        None,
    );
    assert!(a.contains("## Plan Consumed"), "orch path taken");
    assert!(
        !a.contains("axum"),
        "specialised axum block must not appear"
    );
}

#[test]
fn plain_post_users_still_hits_specialised_path() {
    let a = build_artifact("coder", "build a POST /users endpoint", None);
    assert!(a.contains("axum"));
    assert!(a.contains("Uuid::new_v4"));
}

#[test]
fn plain_safe_add_still_hits_specialised_path() {
    let a = build_artifact("coder", "write a safe add function in Rust", None);
    assert!(a.contains("checked_add"));
}

#[test]
fn orch_safe_add_task_bypasses_safe_add_specialisation() {
    let a = build_artifact(
        "coder",
        "[feature orchestration as coder] safe add function",
        None,
    );
    assert!(a.contains("## Plan Consumed"));
    assert!(!a.contains("checked_add"));
}

#[test]
fn orch_prompt_with_schema_does_not_append_structured_output() {
    let schema = json!({"type": "object"});
    let a = build_artifact("coder", "[feature orchestration as coder] x", Some(&schema));
    assert!(a.contains("## Plan Consumed"));
    assert!(!a.contains("Structured Output"));
}

// ── AgentSpawnHandler integration tests ──────────────────────────────────────

#[tokio::test]
async fn coder_orch_spawn_succeeds_and_artifact_exists() {
    let _g = isolate();
    let r = spawn_orch("coder", "[feature orchestration as coder] add payment API").await;
    assert_eq!(r["status"], "completed");
    let path = r["artifact_path"].as_str().unwrap();
    assert!(std::path::Path::new(path).exists());
}

#[tokio::test]
async fn coder_orch_artifact_contains_required_sections() {
    let _g = isolate();
    let r = spawn_orch("coder", "[feature orchestration as coder] add payment API").await;
    let content = std::fs::read_to_string(r["artifact_path"].as_str().unwrap()).unwrap();
    assert!(content.contains("# coder agent"));
    assert!(content.contains("## Plan Consumed"));
    assert!(content.contains("## Implementation"));
}

#[tokio::test]
async fn coder_orch_artifact_bytes_matches_file_size() {
    let _g = isolate();
    let r = spawn_orch("coder", "[feature orchestration as coder] add payment API").await;
    let path = r["artifact_path"].as_str().unwrap();
    let file_len = std::fs::metadata(path).unwrap().len();
    assert_eq!(r["artifact_bytes"].as_u64().unwrap(), file_len);
}

#[tokio::test]
async fn coder_orch_spawn_with_prior_planner_context() {
    let _g = isolate();
    let sep = "\n\nPrevious artifact to consume:\n";
    let prompt = format!(
        "[feature orchestration as coder] add payment API{sep}# planner agent\n## Ordered Delivery Steps\n- coder: implement handler\n"
    );
    let r = spawn_orch("coder", &prompt).await;
    let content = std::fs::read_to_string(r["artifact_path"].as_str().unwrap()).unwrap();
    let idx = content.find("## Plan Consumed").unwrap();
    let section = &content[idx..];
    assert!(section.contains("planner agent"));
    assert!(section.contains("implement handler"));
}

#[tokio::test]
async fn coder_orch_spawn_emits_started_event() {
    let _g = isolate();
    let r = spawn_orch("coder", "[feature orchestration as coder] add payment API").await;
    let events = GovEventsHandler
        .execute(json!({"event_type": "agent.spawn.started", "limit": 10}))
        .await
        .unwrap();
    let agent_id = r["agent_id"].as_str().unwrap();
    let found = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["agent_id"] == agent_id);
    assert!(found, "started event must reference the spawned agent");
}

#[tokio::test]
async fn coder_orch_spawn_emits_completed_event_success() {
    let _g = isolate();
    let r = spawn_orch("coder", "[feature orchestration as coder] add payment API").await;
    let events = GovEventsHandler
        .execute(json!({"event_type": "agent.spawn.completed", "limit": 10}))
        .await
        .unwrap();
    let agent_id = r["agent_id"].as_str().unwrap();
    let evt = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["agent_id"] == agent_id);
    assert!(evt.is_some());
    assert_eq!(evt.unwrap()["payload"]["success"], true);
}

#[tokio::test]
async fn coder_orch_agent_queryable_via_status() {
    let _g = isolate();
    let r = spawn_orch("coder", "[feature orchestration as coder] add payment API").await;
    let id = r["agent_id"].as_str().unwrap();
    let s = AgentStatusHandler
        .execute(json!({"agent_id": id}))
        .await
        .unwrap();
    assert_eq!(s["found"], true);
    assert_eq!(s["archetype"], "coder");
}

#[tokio::test]
async fn coder_orch_agent_appears_in_list() {
    let _g = isolate();
    spawn_orch("coder", "[feature orchestration as coder] add payment API").await;
    let list = AgentStatusHandler.execute(json!({})).await.unwrap();
    assert!(list["count"].as_u64().unwrap() >= 1);
    let has_coder = list["agents"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["archetype"] == "coder");
    assert!(has_coder);
}

#[tokio::test]
async fn coder_orch_message_delivered() {
    let _g = isolate();
    let r = spawn_orch("coder", "[feature orchestration as coder] add payment API").await;
    let id = r["agent_id"].as_str().unwrap();
    let m = AgentMessageHandler
        .execute(json!({"agent_id": id, "message": "implement pagination"}))
        .await
        .unwrap();
    assert_eq!(m["delivered"], true);
    assert_eq!(m["message_count"], 1);
}

#[tokio::test]
async fn sequential_planner_coder_plan_consumed() {
    let _g = isolate();
    let pr = spawn_orch(
        "planner",
        "[feature orchestration as planner] add payment API",
    )
    .await;
    let plan_content = std::fs::read_to_string(pr["artifact_path"].as_str().unwrap()).unwrap();
    let sep = "\n\nPrevious artifact to consume:\n";
    let coder_prompt =
        format!("[feature orchestration as coder] add payment API{sep}{plan_content}");
    let cr = spawn_orch("coder", &coder_prompt).await;
    let coder_content = std::fs::read_to_string(cr["artifact_path"].as_str().unwrap()).unwrap();
    let idx = coder_content.find("## Plan Consumed").unwrap();
    assert!(coder_content[idx..].contains("planner agent"));
}

// ── validation and failure modes ─────────────────────────────────────────────

#[test]
fn validate_rejects_missing_archetype() {
    let err = AgentSpawnHandler.validate(&json!({"prompt": "x", "model": "m"}));
    assert!(err.is_err());
    assert!(err.unwrap_err().to_string().contains("archetype"));
}

#[test]
fn validate_rejects_missing_prompt() {
    let err = AgentSpawnHandler.validate(&json!({"archetype": "coder", "model": "m"}));
    assert!(err.is_err());
    assert!(err.unwrap_err().to_string().contains("prompt"));
}

#[test]
fn validate_rejects_missing_model() {
    let err = AgentSpawnHandler.validate(&json!({"archetype": "coder", "prompt": "x"}));
    assert!(err.is_err());
    assert!(err.unwrap_err().to_string().contains("model"));
}

#[test]
fn validate_rejects_invalid_archetype_in_orch_prompt() {
    let err = AgentSpawnHandler.validate(&json!({
        "archetype": "wizard",
        "prompt": "[feature orchestration as wizard] x",
        "model": "m"
    }));
    assert!(err.is_err());
    assert!(err.unwrap_err().to_string().contains("invalid archetype"));
}

#[tokio::test]
async fn no_runner_env_spawn_succeeds_with_null_stream() {
    let _g = isolate();
    let r = spawn_orch("coder", "[feature orchestration as coder] x").await;
    assert_eq!(r["status"], "completed");
    assert!(r["stream"].is_null());
}

#[tokio::test]
async fn empty_prompt_does_not_panic() {
    let _g = isolate();
    let r = AgentSpawnHandler
        .execute(json!({"archetype": "coder", "prompt": "", "model": "m"}))
        .await
        .unwrap();
    let path = r["artifact_path"].as_str().unwrap();
    assert!(std::fs::metadata(path).unwrap().len() > 0);
}

#[tokio::test]
async fn very_long_task_in_orch_prompt_does_not_panic() {
    let _g = isolate();
    let task = "x".repeat(12_000);
    let prompt = format!("[feature orchestration as coder] {task}");
    let r = AgentSpawnHandler
        .execute(json!({"archetype": "coder", "prompt": prompt, "model": "m"}))
        .await
        .unwrap();
    let path = r["artifact_path"].as_str().unwrap();
    assert!(std::fs::metadata(path).unwrap().len() > 10_000);
}

#[test]
fn parse_template_name_with_spaces_still_parses() {
    // "my feature orchestration as coder" → strip_suffix(" orchestration as coder") = "my feature"
    assert_eq!(
        parse_orch_prompt("coder", "[my feature orchestration as coder] add login"),
        Some(("my feature", "add login"))
    );
}

#[test]
fn parse_coordinator_orch_prompt() {
    assert_eq!(
        parse_orch_prompt(
            "coordinator",
            "[feature orchestration as coordinator] add payment API"
        ),
        Some(("feature", "add payment API"))
    );
}

#[test]
fn coordinator_orch_artifact_contains_sub_agents_section() {
    let a = build_orch_artifact(
        "coordinator",
        "feature",
        "add payment API",
        "[feature orchestration as coordinator] add payment API",
    );
    assert!(a.contains("Sub-agents to dispatch"));
    assert!(!a.contains("## Implementation"));
}

#[test]
fn extract_structured_output_on_orch_artifact_returns_none() {
    let a = coder_orch_artifact();
    assert!(extract_structured_output(&a).is_none());
}
