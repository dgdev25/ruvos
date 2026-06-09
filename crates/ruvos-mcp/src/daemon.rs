//! Relay daemon — Option C of the agent execution bridge (ADR-015).
//!
//! `run_daemon()` starts a background polling loop that:
//! 1. Announces presence on the relay bus with a configurable agent_id
//! 2. Polls this process's relay inbox every `poll_interval_ms` milliseconds
//! 3. Deserialises each message body as JSON and dispatches on `"method"`
//! 4. Dispatches `exec` messages to `ruvos_agent_exec`
//! 5. Stores successful results in memory (`daemon/results/<correlation_id>`)
//!
//! This is the ruflo coordinator pattern: a persistent listener that picks up
//! tasks from the file-based relay bus and executes them with full tool access.

use crate::relay;
use crate::tools::agent_exec::AgentExecHandler;
use crate::tools::handler::ToolHandler;
use crate::tools::memory::MemoryStoreHandler;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time;
use tracing::{info, warn};

const DAEMON_AGENT_ID: &str = "ruvos-daemon";
const DEFAULT_POLL_MS: u64 = 500;
const HEARTBEAT_EVERY_N_POLLS: u32 = 10; // re-announce every 5 s at 500 ms poll

/// Configuration for the relay daemon.
pub struct DaemonConfig {
    /// Relay agent_id — used as the inbox name and presence id.
    pub agent_id: String,
    /// Polling interval in milliseconds.
    pub poll_interval_ms: u64,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            agent_id: DAEMON_AGENT_ID.to_string(),
            poll_interval_ms: DEFAULT_POLL_MS,
        }
    }
}

/// Per-message dispatch outcome.
pub struct DispatchResult {
    pub correlation_id: Option<String>,
    pub success: bool,
    pub output: Value,
}

/// Start the relay daemon.  Runs until the `shutdown` watch flips to `true`.
pub async fn run_daemon(config: DaemonConfig, mut shutdown: tokio::sync::watch::Receiver<bool>) {
    info!("ruvos daemon starting (agent_id={})", config.agent_id);

    let poll_interval = Duration::from_millis(config.poll_interval_ms);
    let mut interval = time::interval(poll_interval);
    let mut poll_count: u32 = 0;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                poll_count += 1;

                // Refresh presence periodically so other agents can find us.
                if poll_count % HEARTBEAT_EVERY_N_POLLS == 1 {
                    let _ = relay::announce(&format!("ruvos daemon ({})", config.agent_id));
                }

                // Drain inbox and dispatch each message.
                match relay::drain_inbox(&config.agent_id) {
                    Ok(messages) => {
                        for relay_msg in messages {
                            // Body is expected to be a JSON object.
                            let body: Value = serde_json::from_str(&relay_msg.body)
                                .unwrap_or(json!({ "method": "unknown" }));
                            let result = dispatch_message(&body).await;
                            if result.success {
                                let key = format!(
                                    "daemon/results/{}",
                                    result.correlation_id.as_deref().unwrap_or("unknown")
                                );
                                let _ = MemoryStoreHandler
                                    .execute(json!({
                                        "key": key,
                                        "value": result.output,
                                        "namespace": "daemon",
                                        "tags": ["daemon", "exec_result"],
                                    }))
                                    .await;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("daemon: inbox drain failed: {:?}", e);
                    }
                }
            }
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    info!("ruvos daemon shutting down");
                    break;
                }
            }
        }
    }
}

/// Dispatch a single message body to the right handler.
pub async fn dispatch_message(msg: &Value) -> DispatchResult {
    let method = msg["method"].as_str().unwrap_or("unknown");
    let correlation_id = msg["correlation_id"].as_str().map(String::from);
    let params = msg.get("params").cloned().unwrap_or(json!({}));

    info!("daemon: dispatching method={method}");

    match method {
        "exec" => match AgentExecHandler.execute(params).await {
            Ok(output) => DispatchResult {
                correlation_id,
                success: output["success"].as_bool().unwrap_or(false),
                output,
            },
            Err(e) => DispatchResult {
                correlation_id,
                success: false,
                output: json!({ "error": e.message() }),
            },
        },
        "ping" => DispatchResult {
            correlation_id,
            success: true,
            output: json!({ "pong": true, "agent_id": DAEMON_AGENT_ID }),
        },
        other => {
            warn!("daemon: unknown method: {other}");
            DispatchResult {
                correlation_id,
                success: false,
                output: json!({ "error": format!("unknown method: {other}") }),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    fn isolate() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(dir.path().to_path_buf());
        dir
    }

    #[test]
    fn daemon_config_defaults() {
        let cfg = DaemonConfig::default();
        assert_eq!(cfg.agent_id, DAEMON_AGENT_ID);
        assert_eq!(cfg.poll_interval_ms, DEFAULT_POLL_MS);
    }

    #[tokio::test]
    async fn dispatch_ping_returns_pong() {
        let _g = isolate();
        let msg = json!({ "method": "ping", "correlation_id": "test-123" });
        let result = dispatch_message(&msg).await;
        assert!(result.success);
        assert_eq!(result.output["pong"], true);
        assert_eq!(result.correlation_id.as_deref(), Some("test-123"));
    }

    #[tokio::test]
    async fn dispatch_exec_runs_echo() {
        let _g = isolate();
        let msg = json!({
            "method": "exec",
            "correlation_id": "exec-abc",
            "params": {
                "ops": [
                    { "op": "run_command", "cmd": "echo", "args": ["daemon test"] }
                ]
            }
        });
        let result = dispatch_message(&msg).await;
        assert!(result.success, "exec should succeed: {:?}", result.output);
        assert_eq!(result.output["ops_executed"], 1);
        assert!(result.output["results"][0]["stdout"]
            .as_str()
            .unwrap_or("")
            .contains("daemon test"));
    }

    #[tokio::test]
    async fn dispatch_unknown_method_returns_error() {
        let _g = isolate();
        let msg = json!({ "method": "teleport" });
        let result = dispatch_message(&msg).await;
        assert!(!result.success);
        assert!(result.output["error"]
            .as_str()
            .unwrap()
            .contains("unknown method"));
    }

    #[tokio::test]
    async fn daemon_runs_and_shuts_down() {
        let _g = isolate();
        let (tx, rx) = tokio::sync::watch::channel(false);
        let cfg = DaemonConfig {
            poll_interval_ms: 50,
            ..Default::default()
        };
        let handle = tokio::spawn(run_daemon(cfg, rx));
        tokio::time::sleep(Duration::from_millis(150)).await;
        let _ = tx.send(true);
        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("daemon did not shut down in time")
            .expect("daemon task panicked");
    }

    #[tokio::test]
    async fn daemon_processes_inbox_message() {
        let _g = isolate();

        // Write a ping message to the daemon's inbox directly.
        crate::paths::ensure_root().unwrap();
        let inbox_dir = crate::paths::relays_dir()
            .join(format!("{}.inbox", DAEMON_AGENT_ID));
        std::fs::create_dir_all(&inbox_dir).unwrap();
        let body = json!({
            "method": "ping",
            "correlation_id": "inbox-test"
        });
        std::fs::write(
            inbox_dir.join("msg001.json"),
            serde_json::to_vec_pretty(&crate::relay::RelayMessage {
                id: "msg001".into(),
                from: "test-sender".into(),
                to: DAEMON_AGENT_ID.into(),
                body: body.to_string(),
                sent_at: chrono::Utc::now().to_rfc3339(),
            })
            .unwrap(),
        )
        .unwrap();

        // Run daemon for a couple of poll cycles.
        let (tx, rx) = tokio::sync::watch::channel(false);
        let cfg = DaemonConfig {
            poll_interval_ms: 30,
            ..Default::default()
        };
        let handle = tokio::spawn(run_daemon(cfg, rx));
        tokio::time::sleep(Duration::from_millis(120)).await;
        let _ = tx.send(true);
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;

        // The result should have been stored in memory.
        let stored = MemoryStoreHandler; // use search to verify
        let search_result = crate::tools::memory::MemorySearchHandler
            .execute(json!({ "query": "daemon/results/inbox-test", "namespace": "daemon" }))
            .await
            .unwrap();
        // At least one result referencing the correlation id should exist.
        let hits = search_result["results"].as_array().unwrap();
        assert!(!hits.is_empty(), "daemon should have stored the result in memory");
    }
}
