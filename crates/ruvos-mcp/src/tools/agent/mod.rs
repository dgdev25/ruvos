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
        assert!(r["structured_output"].is_null(), "no schema = null structured_output");
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

        let o = run_task("a4", "coder", "x", script.to_str(), None).await.unwrap();
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
        let o = run_task("a2", "coder", "x", Some("true"), None).await.unwrap();
        assert!(o.success);
        assert_eq!(o.exit_code, Some(0));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_nonzero_exit_is_failure() {
        let _g = isolate();
        let o = run_task("a3", "tester", "x", Some("false"), None).await.unwrap();
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
}
