//! Agent domain tools (3): spawn, status, message.
//!
//! Agents are persisted to the redb-backed `ruvos-store` (source of truth,
//! survives restarts, signed `.rvf` snapshots) and really execute their task
//! on spawn: each agent produces a real work artifact on disk, and — when
//! `RUVOS_AGENT_RUNNER` is set — additionally runs that command as a real
//! subprocess and captures its output. Every spawn and message also appends an
//! `EventRecord` to the store's audit log (queryable via `gov.events`).
//!
//! In addition to store persistence, each spawned agent is registered with a
//! process-global `InProcessRegistry` so that `agent.message` can deliver
//! messages over a real in-process channel (additive — store persistence is
//! always kept regardless of transport availability).

mod artifact;
mod handlers;
mod task;
mod transport;

pub use handlers::{AgentMessageHandler, AgentSpawnHandler, AgentStatusHandler};

#[cfg(test)]
mod tests {
    use super::handlers::{AgentMessageHandler, AgentSpawnHandler, AgentStatusHandler};
    use super::task::run_task;
    use super::transport::TRANSPORT_REGISTRY;
    use crate::paths;
    use crate::tools::gov::GovEventsHandler;
    use crate::tools::handler::ToolHandler;
    use ruvos_skills::{
        CompressionCodec, SkillChunkLink, SkillPackMeta, SkillRecord, SkillSource, SkillStore,
    };
    use serde_json::json;

    fn isolate() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(dir.path().to_path_buf());
        // Hermetic tests: never auto-detect a real LLM provider in run_task's
        // no-runner path, so outcomes are deterministic regardless of whether
        // claude/gemini/codex is on PATH or OPENROUTER_API_KEY is set.
        std::env::set_var("RUVOS_DISABLE_CLI_ROUTER", "1");
        dir
    }

    async fn spawn(archetype: &str, prompt: &str) -> serde_json::Value {
        AgentSpawnHandler
            .execute(json!({"archetype": archetype, "prompt": prompt, "model": "claude-haiku-4-5"}))
            .await
            .unwrap()
    }

    fn seed_skill_pack() {
        let pack_path = paths::skills_pack_file();
        let store = SkillStore::open(&pack_path).unwrap();
        let skill = SkillRecord {
            id: "safe-rust".to_string(),
            name: "Safe Rust".to_string(),
            version: "1.0.0".to_string(),
            purpose: "Write safe Rust modules and checked arithmetic".to_string(),
            tags: vec![
                "coder".to_string(),
                "rust".to_string(),
                "safety".to_string(),
            ],
            aliases: vec!["safe-rust".to_string()],
            prerequisites: vec!["understand ownership".to_string()],
            safety_level: "advisory".to_string(),
            validation: vec!["compile the module".to_string()],
            summary: Some("Safe Rust implementation guidance".to_string()),
            source: SkillSource {
                source_root: "/skillbase".to_string(),
                source_path: "rust/safe-rust.md".to_string(),
                corpus_hash: "abc123".to_string(),
            },
            created_at: 1,
            updated_at: 1,
        };
        store.put_skill(&skill).unwrap();
        let chunk = store
            .encode_and_put_chunk(
                b"# Safe Rust\nWrite safe Rust modules.",
                CompressionCodec::None,
            )
            .unwrap();
        store.put_chunk(&chunk).unwrap();
        store
            .put_skill_chunks(
                &skill.id,
                &[SkillChunkLink {
                    ordinal: 0,
                    chunk_hash: chunk.hash,
                }],
            )
            .unwrap();
        store
            .put_pack_meta(&SkillPackMeta::new(
                "corpus",
                "/skillbase",
                CompressionCodec::None,
                1,
                1,
            ))
            .unwrap();
    }

    #[tokio::test]
    async fn spawn_with_output_schema_returns_structured_output() {
        let _g = isolate();
        let r = AgentSpawnHandler
            .execute(json!({
                "archetype": "coder",
                "prompt": "build a POST /users endpoint",
                "model": "claude-haiku-4-5",
                "output_schema": {
                    "type": "object",
                    "properties": {
                        "endpoint": { "type": "string" },
                        "method": { "type": "string" }
                    }
                }
            }))
            .await
            .unwrap();
        assert_eq!(r["status"], "completed");
        assert!(
            r["structured_output"].is_object(),
            "structured_output must be present and be an object: {:?}",
            r["structured_output"]
        );
    }

    #[tokio::test]
    async fn spawn_without_output_schema_has_null_structured_output() {
        let _g = isolate();
        let r = AgentSpawnHandler
            .execute(json!({
                "archetype": "tester",
                "prompt": "write unit tests",
                "model": "claude-haiku-4-5"
            }))
            .await
            .unwrap();
        assert_eq!(r["status"], "completed");
        assert!(
            r["structured_output"].is_null(),
            "no schema = null structured_output"
        );
    }

    #[tokio::test]
    async fn spawn_with_explicit_null_output_schema_has_null_structured_output() {
        // Coordinator agents pass `"output_schema": null` to explicitly opt out.
        // Some(Value::Null).is_some() == true, so without the .filter() this
        // would incorrectly treat it as a requested schema and return `{}`.
        let _g = isolate();
        let r = AgentSpawnHandler
            .execute(json!({
                "archetype": "tester",
                "prompt": "write unit tests",
                "model": "claude-haiku-4-5",
                "output_schema": null
            }))
            .await
            .unwrap();
        assert_eq!(r["status"], "completed");
        assert!(
            r["structured_output"].is_null(),
            "explicit null schema must yield null structured_output, got: {}",
            r["structured_output"]
        );
    }

    #[tokio::test]
    async fn spawn_produces_a_real_artifact_file() {
        let _g = isolate();
        let r = spawn("coder", "build a POST /users endpoint").await;
        assert_eq!(r["status"], "completed");
        let path = r["artifact_path"].as_str().unwrap();
        assert!(
            std::path::Path::new(path).exists(),
            "agent must write a real artifact at {}",
            path
        );
        let content = std::fs::read_to_string(path).unwrap();
        assert!(
            content.contains("POST /users"),
            "artifact reflects the task"
        );
        assert!(
            content.contains("coder agent"),
            "artifact reflects archetype"
        );
        assert!(r["artifact_bytes"].as_u64().unwrap() > 0);
    }

    #[tokio::test]
    async fn planner_orchestrates_post_users_feature() {
        let _g = isolate();
        let r = AgentSpawnHandler
            .execute(json!({
                "archetype": "planner",
                "prompt": "[feature orchestration as planner] add POST /users",
                "model": "claude-haiku-4-5"
            }))
            .await
            .unwrap();
        assert_eq!(r["status"], "completed");
        let path = r["artifact_path"].as_str().unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("planner"), "archetype label in artifact");
        assert!(content.contains("POST /users"), "route name in artifact");
        assert!(content.contains("201"), "201 status code documented");
        assert!(content.contains("coder"), "coder downstream agent named");
        assert!(content.contains("tester"), "tester downstream agent named");
        assert!(
            content.contains("reviewer"),
            "reviewer downstream agent named"
        );
    }

    #[tokio::test]
    async fn spawn_publishes_agent_runtime_events() {
        let _g = isolate();
        let r = spawn("tester", "exercise the runtime event path").await;
        assert_eq!(r["status"], "completed");

        let events = GovEventsHandler
            .execute(json!({"event_type": "agent.spawn.completed", "limit": 10}))
            .await
            .unwrap();
        assert!(events["count"].as_u64().unwrap() >= 1);
        assert_eq!(events["events"][0]["agent_id"], r["agent_id"]);
        assert_eq!(events["events"][0]["payload"]["agent_id"], r["agent_id"]);
    }

    #[tokio::test]
    async fn spawn_uses_selected_skills_from_pack() {
        let _g = isolate();
        seed_skill_pack();
        let r = spawn("coder", "write a safe rust module").await;
        assert_eq!(r["status"], "completed");
        let selected = r["selected_skills"].as_object().expect("selected skills");
        assert_eq!(selected["selections"][0]["skill_id"], "safe-rust");
        let path = r["artifact_path"].as_str().unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("Skill guidance"));
        assert!(content.contains("safe-rust"));
    }

    #[tokio::test]
    async fn no_runner_defaults_to_success() {
        let _g = isolate();
        let o = run_task("a1", "coder", "x", None, None).await.unwrap();
        assert!(o.success, "no executor → assumed success");
        assert_eq!(o.exit_code, None);
        assert!(o.stream.is_none(), "no runner → no stream analysis");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_stdout_is_streamed_and_observed() {
        use std::os::unix::fs::PermissionsExt;
        let g = isolate();
        let script = g.path().join("multi.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nfor i in 1 2 3 4 5 6 7 8 9 10 11 12; do echo \"line$i\"; done\nexit 0\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

        let o = run_task("a4", "coder", "x", script.to_str(), None)
            .await
            .unwrap();
        assert!(o.success);
        let (observed, _anomalies) = o.stream.expect("runner stream summary");
        assert!(
            observed >= 12,
            "every streamed line is observed, got {observed}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_exit_zero_is_success() {
        let _g = isolate();
        let o = run_task("a2", "coder", "x", Some("true"), None)
            .await
            .unwrap();
        assert!(o.success);
        assert_eq!(o.exit_code, Some(0));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_nonzero_exit_is_failure() {
        let _g = isolate();
        let o = run_task("a3", "tester", "x", Some("false"), None)
            .await
            .unwrap();
        assert!(!o.success, "non-zero exit must be a failure");
        assert_eq!(o.exit_code, Some(1));
    }

    #[tokio::test]
    async fn spawn_rejects_invalid_archetype() {
        let _g = isolate();
        let err = AgentSpawnHandler.validate(&json!({
            "archetype": "wizard", "prompt": "x", "model": "m"
        }));
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn status_lists_and_queries_persisted_agents() {
        let _g = isolate();
        let a = spawn("tester", "write tests").await;
        let id = a["agent_id"].as_str().unwrap().to_string();

        let list = AgentStatusHandler.execute(json!({})).await.unwrap();
        assert_eq!(list["count"], 1);

        let one = AgentStatusHandler
            .execute(json!({"agent_id": id}))
            .await
            .unwrap();
        assert_eq!(one["found"], true);
        assert_eq!(one["archetype"], "tester");
    }

    #[tokio::test]
    async fn message_appends_to_persisted_agent() {
        let _g = isolate();
        let a = spawn("coordinator", "coordinate").await;
        let id = a["agent_id"].as_str().unwrap().to_string();

        let m = AgentMessageHandler
            .execute(json!({"agent_id": id.clone(), "message": "add pagination"}))
            .await
            .unwrap();
        assert_eq!(m["delivered"], true);
        assert_eq!(m["message_count"], 1);

        let one = AgentStatusHandler
            .execute(json!({"agent_id": id}))
            .await
            .unwrap();
        assert_eq!(one["message_count"], 1);
    }

    #[tokio::test]
    async fn message_to_unknown_agent_fails() {
        let _g = isolate();
        let m = AgentMessageHandler
            .execute(json!({"agent_id": "nope", "message": "hi"}))
            .await
            .unwrap();
        assert_eq!(m["delivered"], false);
    }

    #[tokio::test]
    async fn runner_executes_real_subprocess() {
        let _g = isolate();
        let o = run_task("r1", "docs", "document the API", Some("echo"), None)
            .await
            .unwrap();
        assert!(o.success);
        assert!(o.result.contains("docs"), "runner stdout: {}", o.result);
        assert!(o.result.contains("document the API"));
        assert!(o.stream.is_some(), "runner output is stream-analyzed");
    }

    // Verify TRANSPORT_REGISTRY is accessible (used by AgentStatusHandler internally).
    #[test]
    fn transport_registry_is_accessible() {
        let _ = TRANSPORT_REGISTRY.contains("nonexistent");
    }

    // ── AgentSpawnHandler::validate ───────────────────────────────────────────

    #[test]
    fn validate_spawn_rejects_missing_prompt() {
        let err = AgentSpawnHandler.validate(&json!({"archetype": "coder", "model": "m"}));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("prompt"));
    }

    #[test]
    fn validate_spawn_rejects_missing_model() {
        let err = AgentSpawnHandler.validate(&json!({"archetype": "coder", "prompt": "x"}));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("model"));
    }

    #[test]
    fn validate_spawn_rejects_missing_archetype() {
        let err = AgentSpawnHandler.validate(&json!({"prompt": "x", "model": "m"}));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("archetype"));
    }

    #[test]
    fn validate_spawn_rejects_invalid_archetype() {
        let err = AgentSpawnHandler
            .validate(&json!({"archetype": "wizard", "prompt": "x", "model": "m"}));
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(msg.contains("wizard"), "error must name the bad archetype");
    }

    #[test]
    fn validate_spawn_accepts_all_twelve_archetypes() {
        use super::artifact::VALID_ARCHETYPES;
        for archetype in VALID_ARCHETYPES {
            let r = AgentSpawnHandler.validate(&json!({
                "archetype": archetype, "prompt": "x", "model": "m"
            }));
            assert!(r.is_ok(), "archetype '{archetype}' must be valid");
        }
    }

    // ── AgentMessageHandler::validate ────────────────────────────────────────

    #[test]
    fn validate_message_rejects_missing_agent_id() {
        let err = AgentMessageHandler.validate(&json!({"message": "hi"}));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("agent_id"));
    }

    #[test]
    fn validate_message_rejects_missing_message_field() {
        let err = AgentMessageHandler.validate(&json!({"agent_id": "abc"}));
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("message"));
    }

    #[test]
    fn validate_message_accepts_both_required_fields() {
        assert!(AgentMessageHandler
            .validate(&json!({"agent_id": "x", "message": "hi"}))
            .is_ok());
    }

    #[test]
    fn validate_status_always_passes() {
        assert!(AgentStatusHandler.validate(&json!({})).is_ok());
        assert!(AgentStatusHandler
            .validate(&json!({"agent_id": "any"}))
            .is_ok());
    }

    // ── Integration gap tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn status_returns_not_found_for_unknown_agent_id() {
        let _g = isolate();
        let r = AgentStatusHandler
            .execute(json!({"agent_id": "00000000-0000-0000-0000-000000000000"}))
            .await
            .unwrap();
        assert_eq!(r["found"], false);
    }

    #[tokio::test]
    async fn multiple_messages_increment_count_sequentially() {
        let _g = isolate();
        let a = spawn("coordinator", "coordinate agents").await;
        let id = a["agent_id"].as_str().unwrap().to_string();
        for i in 1..=3_u64 {
            let m = AgentMessageHandler
                .execute(json!({"agent_id": id.clone(), "message": format!("msg {i}")}))
                .await
                .unwrap();
            assert_eq!(m["message_count"], i, "count must increment per delivery");
        }
        let status = AgentStatusHandler
            .execute(json!({"agent_id": id}))
            .await
            .unwrap();
        assert_eq!(status["message_count"], 3);
    }

    #[tokio::test]
    async fn spawn_multiple_agents_all_appear_in_list() {
        let _g = isolate();
        spawn("coder", "task A").await;
        spawn("tester", "task B").await;
        spawn("reviewer", "task C").await;
        let list = AgentStatusHandler.execute(json!({})).await.unwrap();
        assert_eq!(list["count"], 3);
        assert_eq!(list["agents"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn spawn_reported_bytes_matches_file_on_disk() {
        let _g = isolate();
        let r = spawn("perf", "profile the hot path").await;
        let reported = r["artifact_bytes"].as_u64().unwrap();
        let path = r["artifact_path"].as_str().unwrap();
        let actual = std::fs::metadata(path).unwrap().len();
        assert_eq!(
            reported, actual,
            "artifact_bytes must match the real file size"
        );
    }

    #[tokio::test]
    async fn spawn_sets_agent_id_as_valid_uuid() {
        let _g = isolate();
        let r = spawn("researcher", "investigate the codebase").await;
        let id = r["agent_id"].as_str().unwrap();
        assert!(
            uuid::Uuid::parse_str(id).is_ok(),
            "agent_id must be a valid UUID v4, got: {id}"
        );
    }

    #[tokio::test]
    async fn spawn_stream_is_null_when_no_runner_configured() {
        let _g = isolate();
        let r = spawn("security", "audit the API surface").await;
        assert!(
            r["stream"].is_null(),
            "no RUVOS_AGENT_RUNNER → stream field must be null"
        );
    }
}

#[cfg(test)]
mod tests_orch;
