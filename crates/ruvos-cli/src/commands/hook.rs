//! `ruvos hook <kind> --phase <pre|post>` — harness-driven hook entry point.
//!
//! Claude Code hooks invoke this binary with the hook event JSON on stdin.
//! The event is dispatched through the same handlers the MCP tools use, so
//! SONA learning / auto-swarm / event log fire whether or not the model
//! remembers to call ruvos_hooks_pre itself. Always exits 0 on dispatch
//! errors (a learning-layer failure must never block the user's edit).

use anyhow::{bail, Result};
use ruvos_mcp::tools::handler::ToolHandler;
use ruvos_mcp::tools::hooks::{HooksPostHandler, HooksPreHandler};
use serde_json::{json, Value};

pub async fn run_hook(kind: &str, phase: &str, event: Value) -> Result<Value> {
    if !matches!(kind, "task" | "edit" | "command" | "session") {
        bail!("unknown hook kind '{kind}' (task|edit|command|session)");
    }
    let result = match phase {
        "pre" => {
            HooksPreHandler::new()
                .execute(json!({"kind": kind, "payload": event}))
                .await
        }
        "post" => {
            HooksPostHandler::new()
                .execute(json!({
                    "kind": kind,
                    "payload": event,
                    // The harness doesn't carry success; default true and let
                    // the payload's exit/error fields inform learning.
                    "success": event.get("success").and_then(|v| v.as_bool()).unwrap_or(true),
                }))
                .await
        }
        other => bail!("unknown phase '{other}' (pre|post)"),
    };
    result.map_err(|e| anyhow::anyhow!("hook dispatch failed: {}", e.message()))
}

/// CLI entry: read the hook event JSON from stdin, dispatch, print result.
pub async fn run_from_stdin(kind: &str, phase: &str) -> Result<()> {
    let mut input = String::new();
    use std::io::Read;
    std::io::stdin().read_to_string(&mut input)?;
    let event: Value = serde_json::from_str(&input).unwrap_or(json!({}));
    match run_hook(kind, phase, event).await {
        Ok(out) => println!("{out}"),
        // Never fail the user's action because the learning layer hiccuped.
        Err(e) => eprintln!("ruvos hook: {e}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dispatches_pre_task_hook() {
        let _guard = crate::commands::ruvos_home_lock().await;
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("RUVOS_HOME", dir.path());
        let out = run_hook(
            "task",
            "pre",
            serde_json::json!({"prompt": "implement an endpoint"}),
        )
        .await
        .unwrap();
        assert!(out.get("status").is_some());
    }

    #[tokio::test]
    async fn rejects_unknown_kind() {
        let _guard = crate::commands::ruvos_home_lock().await;
        assert!(run_hook("bogus", "pre", serde_json::json!({}))
            .await
            .is_err());
    }
}
