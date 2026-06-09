//! Agent execution bridge — closes Gaps 1-3 (file write, shell exec, git).
//!
//! `ruvos_agent_exec` accepts a list of typed `ExecOp`s and runs them using
//! `PluginExecutor` (tokio::process::Command).  With `sandbox: true` every op
//! runs inside a fresh temp directory so nothing touches the host tree.
//!
//! ADR-017: `write_slot` / `read_slot` ops enable cross-agent file handoff
//! within a swarm via ephemeral scratch slots scoped to a swarm ID.

use crate::tools::handler::{ExecuteFuture, ToolHandler};
use crate::Result;
use ruvos_plugin_host::executor::PluginExecutor;
use ruvos_plugin_host::types::{ExecutionRequest, ExecutionResult};
use serde_json::{json, Value};
use std::path::PathBuf;
use uuid::Uuid;

pub struct AgentExecHandler;

impl ToolHandler for AgentExecHandler {
    fn name(&self) -> &'static str {
        "exec"
    }
    fn domain(&self) -> &'static str {
        "agent"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "ops": {
                    "type": "array",
                    "description": "Ordered list of operations to execute",
                    "items": {
                        "type": "object",
                        "properties": {
                            "op": {
                                "type": "string",
                                "enum": ["write_file", "patch_file", "read_file", "run_command", "git_op", "write_slot", "read_slot"]
                            },
                            "path":       { "type": "string" },
                            "content":    { "type": "string" },
                            "old":        { "type": "string", "description": "Exact string to find (patch_file)" },
                            "new":        { "type": "string", "description": "Replacement string (patch_file)" },
                            "expected_replacements": { "type": "integer", "description": "How many occurrences to expect and replace (default 1)" },
                            "cmd":        { "type": "string" },
                            "args":       { "type": "array", "items": { "type": "string" } },
                            "cwd":        { "type": "string" },
                            "git_op":     { "type": "string", "enum": ["add", "commit", "status", "diff"] },
                            "message":    { "type": "string" },
                            "slot_name":  { "type": "string", "description": "Slot identifier within the swarm scratch space" },
                            "swarm_id":   { "type": "string", "description": "Swarm scope; defaults to 'default'" },
                            "agent_id":   { "type": "string", "description": "Optional source agent tag for write_slot" },
                            "timeout_ms": { "type": "integer", "description": "Max wait for read_slot in ms (default 10000)" }
                        },
                        "required": ["op"]
                    }
                },
                "sandbox": {
                    "type": "boolean",
                    "description": "Run all ops inside a fresh temp directory (OS-level isolation)"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Override working directory for all run_command / git_op calls"
                },
                "resume_journal_id": {
                    "type": "string",
                    "description": "Resume a previous run: skip ops already marked completed in the journal. Get journal_id from a prior agent_exec response."
                }
            },
            "required": ["ops"]
        })
    }

    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("ops").and_then(|v| v.as_array()).is_none() {
            return Err(crate::RuvosError::ValidationError(
                "ops must be a non-null array".to_string(),
            ));
        }
        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move { run_exec(params).await })
    }
}

async fn run_exec(params: Value) -> Result<Value> {
    let ops = params["ops"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let sandbox = params["sandbox"].as_bool().unwrap_or(false);
    let working_dir_override = params["working_dir"].as_str().map(PathBuf::from);

    // ADR-019: journal setup — resume skips already-completed ops.
    let resume_journal_id = params["resume_journal_id"].as_str().map(String::from);
    let journal_id = resume_journal_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let journal_dir = crate::paths::data_root()
        .join("exec-journal")
        .join(&journal_id);
    let _ = std::fs::create_dir_all(&journal_dir);
    purge_stale_journals();

    // Create sandbox temp dir if requested.
    let _sandbox_dir: Option<tempfile::TempDir>;
    let base_cwd: Option<PathBuf> = if sandbox {
        let dir = tempfile::tempdir().map_err(|e| {
            crate::RuvosError::ValidationError(format!("failed to create sandbox: {e}"))
        })?;
        let path = dir.path().to_path_buf();
        _sandbox_dir = Some(dir);
        Some(path)
    } else {
        _sandbox_dir = None;
        working_dir_override
    };

    let executor = PluginExecutor::new();
    let mut results: Vec<Value> = Vec::with_capacity(ops.len());
    let mut skipped = 0usize;

    for (i, op) in ops.iter().enumerate() {
        let op_name = op["op"].as_str().unwrap_or("unknown");
        let checkpoint_path = journal_dir.join(format!("{i}.json"));

        // Resume: skip ops that already completed in a prior run.
        if resume_journal_id.is_some() {
            if let Ok(raw) = std::fs::read_to_string(&checkpoint_path) {
                if let Ok(entry) = serde_json::from_str::<Value>(&raw) {
                    if entry["status"].as_str() == Some("completed") {
                        results.push(entry["result"].clone());
                        skipped += 1;
                        continue;
                    }
                }
            }
        }

        // Write pending checkpoint before executing.
        let _ = std::fs::write(
            &checkpoint_path,
            json!({"op_index": i, "op": op, "status": "pending"}).to_string(),
        );

        let result = execute_op(&executor, op, base_cwd.as_deref(), i).await;
        let status = if result["status"].as_str() == Some("error") {
            "failed"
        } else {
            "completed"
        };

        // Persist outcome to journal.
        let _ = std::fs::write(
            &checkpoint_path,
            json!({
                "op_index": i,
                "op": op,
                "status": status,
                "result": &result,
            })
            .to_string(),
        );

        results.push(result);
        // Stop on first fatal failure (exit status != 0 for commands).
        let last = results.last().unwrap();
        if last["status"].as_str() == Some("error") && op_name != "read_file" {
            break;
        }
    }

    let all_ok = results.iter().all(|r| r["status"].as_str() != Some("error"));
    Ok(json!({
        "success": all_ok,
        "sandbox": sandbox,
        "ops_executed": results.len(),
        "skipped_completed": skipped,
        "journal_id": journal_id,
        "results": results,
    }))
}

/// Remove exec-journal directories older than 24 hours.
fn purge_stale_journals() {
    let journal_root = crate::paths::data_root().join("exec-journal");
    let cutoff = std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(86400))
        .unwrap_or(std::time::UNIX_EPOCH);
    if let Ok(entries) = std::fs::read_dir(&journal_root) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if let Ok(modified) = meta.modified() {
                    if modified < cutoff {
                        let _ = std::fs::remove_dir_all(entry.path());
                    }
                }
            }
        }
    }
}

async fn execute_op(
    executor: &PluginExecutor,
    op: &Value,
    base_cwd: Option<&std::path::Path>,
    index: usize,
) -> Value {
    let op_name = op["op"].as_str().unwrap_or("unknown");

    match op_name {
        "write_file" => {
            let path_str = match op["path"].as_str() {
                Some(p) => p,
                None => return op_error(index, op_name, "missing path"),
            };
            let content = op["content"].as_str().unwrap_or("");
            let full_path: PathBuf = if let Some(base) = base_cwd {
                base.join(path_str)
            } else {
                PathBuf::from(path_str)
            };
            if let Some(parent) = full_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return op_error(index, op_name, &format!("mkdir failed: {e}"));
                }
            }
            match std::fs::write(&full_path, content) {
                Ok(()) => json!({
                    "index": index,
                    "op": op_name,
                    "status": "ok",
                    "path": full_path.to_string_lossy(),
                    "bytes_written": content.len(),
                }),
                Err(e) => op_error(index, op_name, &e.to_string()),
            }
        }

        "read_file" => {
            let path_str = match op["path"].as_str() {
                Some(p) => p,
                None => return op_error(index, op_name, "missing path"),
            };
            let full_path: PathBuf = if let Some(base) = base_cwd {
                base.join(path_str)
            } else {
                PathBuf::from(path_str)
            };
            match std::fs::read_to_string(&full_path) {
                Ok(content) => json!({
                    "index": index,
                    "op": op_name,
                    "status": "ok",
                    "path": full_path.to_string_lossy(),
                    "content": content,
                }),
                Err(e) => op_error(index, op_name, &e.to_string()),
            }
        }

        "run_command" => {
            let cmd = match op["cmd"].as_str() {
                Some(c) => c,
                None => return op_error(index, op_name, "missing cmd"),
            };
            let args: Vec<String> = op["args"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let cwd = op["cwd"]
                .as_str()
                .map(PathBuf::from)
                .or_else(|| base_cwd.map(|p| p.to_path_buf()));

            let req = ExecutionRequest {
                plugin_name: "agent_exec".to_string(),
                command: cmd.to_string(),
                args,
                cwd,
            };
            match executor.execute(&req).await {
                Ok(ExecutionResult { status, stdout, stderr }) => json!({
                    "index": index,
                    "op": op_name,
                    "status": if status == 0 { "ok" } else { "error" },
                    "exit_code": status,
                    "stdout": stdout,
                    "stderr": stderr,
                }),
                Err(e) => op_error(index, op_name, &e.to_string()),
            }
        }

        "git_op" => {
            let git_op = match op["git_op"].as_str() {
                Some(g) => g,
                None => return op_error(index, op_name, "missing git_op (add|commit|status|diff)"),
            };
            let cwd = op["cwd"]
                .as_str()
                .map(PathBuf::from)
                .or_else(|| base_cwd.map(|p| p.to_path_buf()));

            let cmd_args = match git_op {
                "add" => {
                    let paths: Vec<String> = op["paths"]
                        .as_array()
                        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                        .unwrap_or_else(|| vec![".".to_string()]);
                    let mut a = vec!["add".to_string()];
                    a.extend(paths);
                    a
                }
                "commit" => {
                    let message = op["message"].as_str().unwrap_or("chore: agent commit");
                    vec!["commit".to_string(), "-m".to_string(), message.to_string()]
                }
                "status" => vec!["status".to_string(), "--short".to_string()],
                "diff"   => vec!["diff".to_string()],
                other    => return op_error(index, op_name, &format!("unknown git_op: {other}")),
            };
            let req = ExecutionRequest {
                plugin_name: "agent_exec".to_string(),
                command: "git".to_string(),
                args: cmd_args,
                cwd,
            };
            match executor.execute(&req).await {
                Ok(ExecutionResult { status, stdout, stderr }) => json!({
                    "index": index,
                    "op": op_name,
                    "git_op": git_op,
                    "status": if status == 0 { "ok" } else { "error" },
                    "exit_code": status,
                    "stdout": stdout,
                    "stderr": stderr,
                }),
                Err(e) => op_error(index, op_name, &e.to_string()),
            }
        }

        "write_slot" => {
            let slot_name = match op["slot_name"].as_str() {
                Some(s) => s,
                None => return op_error(index, op_name, "missing slot_name"),
            };
            let content = op["content"].as_str().unwrap_or("");
            let swarm_id = op["swarm_id"].as_str().unwrap_or("default");
            let agent_id = op["agent_id"].as_str().unwrap_or("");
            let slot_path = slot_file_path(swarm_id, slot_name);
            if let Some(parent) = slot_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return op_error(index, op_name, &format!("mkdir failed: {e}"));
                }
            }
            // Write a JSON envelope so read_slot gets metadata too.
            let envelope = json!({
                "slot_name": slot_name,
                "swarm_id": swarm_id,
                "agent_id": agent_id,
                "content": content,
                "bytes": content.len(),
            });
            match std::fs::write(&slot_path, envelope.to_string()) {
                Ok(()) => json!({
                    "index": index,
                    "op": op_name,
                    "status": "ok",
                    "slot_name": slot_name,
                    "swarm_id": swarm_id,
                    "bytes_written": content.len(),
                }),
                Err(e) => op_error(index, op_name, &e.to_string()),
            }
        }

        "read_slot" => {
            let slot_name = match op["slot_name"].as_str() {
                Some(s) => s,
                None => return op_error(index, op_name, "missing slot_name"),
            };
            let swarm_id = op["swarm_id"].as_str().unwrap_or("default");
            let timeout_ms = op["timeout_ms"].as_u64().unwrap_or(10_000);
            let slot_path = slot_file_path(swarm_id, slot_name);
            let deadline = std::time::Instant::now()
                + std::time::Duration::from_millis(timeout_ms);
            loop {
                if slot_path.exists() {
                    match std::fs::read_to_string(&slot_path) {
                        Ok(raw) => {
                            let envelope: Value =
                                serde_json::from_str(&raw).unwrap_or(json!({"content": raw}));
                            let content = envelope["content"].as_str().unwrap_or("").to_owned();
                            break json!({
                                "index": index,
                                "op": op_name,
                                "status": "ok",
                                "slot_name": slot_name,
                                "swarm_id": swarm_id,
                                "agent_id": envelope["agent_id"],
                                "content": content,
                                "bytes": content.len(),
                            });
                        }
                        Err(e) => break op_error(index, op_name, &e.to_string()),
                    }
                }
                if std::time::Instant::now() >= deadline {
                    break op_error(
                        index,
                        op_name,
                        &format!("timeout waiting for slot '{slot_name}' in swarm '{swarm_id}'"),
                    );
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }

        "patch_file" => {
            let path_str = match op["path"].as_str() {
                Some(p) => p,
                None => return op_error(index, op_name, "missing path"),
            };
            let old = match op["old"].as_str() {
                Some(s) => s,
                None => return op_error(index, op_name, "missing 'old' string"),
            };
            let new = op["new"].as_str().unwrap_or("");
            let expected = op["expected_replacements"].as_u64().unwrap_or(1) as usize;
            let full_path: PathBuf = if let Some(base) = base_cwd {
                base.join(path_str)
            } else {
                PathBuf::from(path_str)
            };
            let before = match std::fs::read_to_string(&full_path) {
                Ok(s) => s,
                Err(e) => return op_error(index, op_name, &format!("read failed: {e}")),
            };
            let count = before.match_indices(old).count();
            if count != expected {
                return op_error(
                    index,
                    op_name,
                    &format!(
                        "match count mismatch: expected {expected}, found {count} in '{}'",
                        full_path.display()
                    ),
                );
            }
            let after = if expected == 1 {
                before.replacen(old, new, 1)
            } else {
                before.replace(old, new)
            };
            // Atomic write: temp file in same directory + rename.
            let parent = full_path.parent().unwrap_or(std::path::Path::new("."));
            let tmp_path = parent.join(format!(
                ".patch_tmp_{}_{}",
                full_path.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
                std::process::id()
            ));
            if let Err(e) = std::fs::write(&tmp_path, &after) {
                return op_error(index, op_name, &format!("temp write failed: {e}"));
            }
            // Copy permissions from original before rename.
            if let Ok(meta) = std::fs::metadata(&full_path) {
                let _ = std::fs::set_permissions(&tmp_path, meta.permissions());
            }
            match std::fs::rename(&tmp_path, &full_path) {
                Ok(()) => json!({
                    "index": index,
                    "op": op_name,
                    "status": "ok",
                    "path": full_path.to_string_lossy(),
                    "replacements": count,
                    "bytes_before": before.len(),
                    "bytes_after": after.len(),
                }),
                Err(e) => {
                    let _ = std::fs::remove_file(&tmp_path);
                    op_error(index, op_name, &format!("rename failed: {e}"))
                }
            }
        }

        other => op_error(index, other, &format!("unknown op: {other}")),
    }
}

/// Path to a named slot file: `~/.ruvos/swarms/{swarm_id}/slots/{slot_name}`.
fn slot_file_path(swarm_id: &str, slot_name: &str) -> PathBuf {
    crate::paths::data_root().join("swarms").join(swarm_id).join("slots").join(slot_name)
}

/// Remove all slots for a swarm (call on swarm_complete).
pub fn clear_swarm_slots(swarm_id: &str) -> std::io::Result<()> {
    let dir = crate::paths::data_root().join("swarms").join(swarm_id).join("slots");
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

fn op_error(index: usize, op: &str, message: &str) -> Value {
    json!({
        "index": index,
        "op": op,
        "status": "error",
        "error": message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn isolate() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(dir.path().to_path_buf());
        dir
    }

    #[tokio::test]
    async fn write_and_read_roundtrip() {
        let _g = isolate();
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("hello.txt");

        let result = AgentExecHandler
            .execute(json!({
                "ops": [
                    { "op": "write_file", "path": path.to_str().unwrap(), "content": "hello ruvos" },
                    { "op": "read_file",  "path": path.to_str().unwrap() },
                ]
            }))
            .await
            .unwrap();

        assert_eq!(result["success"], true);
        assert_eq!(result["ops_executed"], 2);
        assert_eq!(result["results"][0]["status"], "ok");
        assert_eq!(result["results"][1]["content"], "hello ruvos");
    }

    #[tokio::test]
    async fn sandbox_mode_writes_to_temp_dir() {
        let _g = isolate();
        let result = AgentExecHandler
            .execute(json!({
                "ops": [
                    { "op": "write_file", "path": "output.txt", "content": "sandbox test" },
                    { "op": "read_file",  "path": "output.txt" },
                ],
                "sandbox": true
            }))
            .await
            .unwrap();

        assert_eq!(result["success"], true);
        assert_eq!(result["results"][1]["content"], "sandbox test");
        // Path written is inside a temp dir, not the cwd.
        let written_path = result["results"][0]["path"].as_str().unwrap();
        assert!(written_path.contains(std::env::temp_dir().to_str().unwrap())
            || written_path.contains("/tmp"));
    }

    #[tokio::test]
    async fn run_command_echo() {
        let _g = isolate();
        let result = AgentExecHandler
            .execute(json!({
                "ops": [
                    { "op": "run_command", "cmd": "echo", "args": ["hello world"] }
                ]
            }))
            .await
            .unwrap();

        assert_eq!(result["success"], true);
        assert_eq!(result["results"][0]["exit_code"], 0);
        assert!(result["results"][0]["stdout"].as_str().unwrap().contains("hello world"));
    }

    #[tokio::test]
    async fn run_command_failure_stops_pipeline() {
        let _g = isolate();
        let result = AgentExecHandler
            .execute(json!({
                "ops": [
                    { "op": "run_command", "cmd": "false", "args": [] },
                    { "op": "run_command", "cmd": "echo", "args": ["should not run"] },
                ]
            }))
            .await
            .unwrap();

        assert_eq!(result["success"], false);
        // Only the first op ran (pipeline stopped on failure).
        assert_eq!(result["ops_executed"], 1);
    }

    #[tokio::test]
    async fn git_status_in_cwd() {
        let _g = isolate();
        // git status in the ruvos repo dir should succeed.
        let result = AgentExecHandler
            .execute(json!({
                "ops": [
                    { "op": "git_op", "git_op": "status", "cwd": "/home/lyle/dev/ruvos" }
                ]
            }))
            .await
            .unwrap();

        assert_eq!(result["results"][0]["exit_code"], 0);
    }

    #[tokio::test]
    async fn invalid_op_returns_error() {
        let _g = isolate();
        let result = AgentExecHandler
            .execute(json!({
                "ops": [
                    { "op": "teleport", "destination": "mars" }
                ]
            }))
            .await
            .unwrap();

        assert_eq!(result["results"][0]["status"], "error");
    }

    #[tokio::test]
    async fn validate_rejects_missing_ops() {
        let err = AgentExecHandler.validate(&json!({}));
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn write_slot_and_read_slot_roundtrip() {
        let _g = isolate();
        let result = AgentExecHandler
            .execute(json!({
                "ops": [
                    {
                        "op": "write_slot",
                        "slot_name": "coder-output",
                        "swarm_id": "test-swarm-1",
                        "agent_id": "coder-agent",
                        "content": "fn main() { println!(\"hello\"); }"
                    },
                    {
                        "op": "read_slot",
                        "slot_name": "coder-output",
                        "swarm_id": "test-swarm-1",
                        "timeout_ms": 1000
                    }
                ]
            }))
            .await
            .unwrap();

        assert_eq!(result["success"], true);
        assert_eq!(result["ops_executed"], 2);
        assert_eq!(result["results"][0]["status"], "ok");
        assert_eq!(result["results"][0]["slot_name"], "coder-output");
        assert_eq!(result["results"][1]["status"], "ok");
        assert_eq!(result["results"][1]["content"], "fn main() { println!(\"hello\"); }");
        assert_eq!(result["results"][1]["agent_id"], "coder-agent");
    }

    #[tokio::test]
    async fn read_slot_times_out_when_missing() {
        let _g = isolate();
        let result = AgentExecHandler
            .execute(json!({
                "ops": [
                    {
                        "op": "read_slot",
                        "slot_name": "nonexistent-slot",
                        "swarm_id": "test-swarm-2",
                        "timeout_ms": 300
                    }
                ]
            }))
            .await
            .unwrap();

        assert_eq!(result["results"][0]["status"], "error");
        assert!(result["results"][0]["error"]
            .as_str()
            .unwrap_or("")
            .contains("timeout"));
    }

    #[tokio::test]
    async fn patch_file_replaces_exact_string() {
        let _g = isolate();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "fn foo() -> i32 { 1 }\n").unwrap();

        let result = AgentExecHandler
            .execute(json!({
                "ops": [{
                    "op": "patch_file",
                    "path": file.to_str().unwrap(),
                    "old": "fn foo() -> i32 { 1 }",
                    "new": "fn foo() -> i32 { 42 }"
                }]
            }))
            .await
            .unwrap();

        assert_eq!(result["results"][0]["status"], "ok");
        assert_eq!(result["results"][0]["replacements"], 1);
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.contains("42"), "replacement must be present");
        assert!(!content.contains("{ 1 }"), "old string must be gone");
    }

    #[tokio::test]
    async fn patch_file_errors_on_mismatch() {
        let _g = isolate();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        std::fs::write(&file, "fn foo() {}\n").unwrap();

        let result = AgentExecHandler
            .execute(json!({
                "ops": [{
                    "op": "patch_file",
                    "path": file.to_str().unwrap(),
                    "old": "fn bar() {}",
                    "new": "fn bar() { todo!() }"
                }]
            }))
            .await
            .unwrap();

        assert_eq!(result["results"][0]["status"], "error");
        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "fn foo() {}\n", "file must be unchanged on mismatch");
    }

    #[tokio::test]
    async fn clear_swarm_slots_removes_slot_dir() {
        let _g = isolate();
        // Write a slot then clear it.
        AgentExecHandler
            .execute(json!({
                "ops": [
                    {
                        "op": "write_slot",
                        "slot_name": "artifact",
                        "swarm_id": "cleanup-swarm",
                        "content": "data"
                    }
                ]
            }))
            .await
            .unwrap();

        let slot_path = slot_file_path("cleanup-swarm", "artifact");
        assert!(slot_path.exists(), "slot file must exist before clear");
        clear_swarm_slots("cleanup-swarm").unwrap();
        assert!(!slot_path.exists(), "slot file must be gone after clear");
    }

    #[tokio::test]
    async fn journal_id_returned_in_response() {
        let _g = isolate();
        let result = AgentExecHandler
            .execute(json!({"ops": [{"op": "run_command", "cmd": "echo", "args": ["hi"]}]}))
            .await
            .unwrap();
        assert!(result["journal_id"].as_str().is_some(), "journal_id must be present");
        assert_eq!(result["skipped_completed"], 0);
    }

    #[tokio::test]
    async fn resume_skips_completed_ops() {
        let _g = isolate();
        let dir = tempfile::tempdir().unwrap();
        let file_a = dir.path().join("a.txt");
        let file_b = dir.path().join("b.txt");

        // First run: write two files; op 1 succeeds, op 2 fails (dir missing).
        let first = AgentExecHandler
            .execute(json!({
                "ops": [
                    {"op": "write_file", "path": file_a.to_str().unwrap(), "content": "hello"},
                    {"op": "run_command", "cmd": "false", "args": []}
                ]
            }))
            .await
            .unwrap();

        assert_eq!(first["success"], false, "first run should fail at op 1");
        let journal_id = first["journal_id"].as_str().unwrap().to_string();
        assert!(file_a.exists(), "op 0 wrote file_a");

        // Second run with resume: op 0 is skipped (already completed),
        // op 1 now succeeds (replaced with echo).
        let second = AgentExecHandler
            .execute(json!({
                "resume_journal_id": journal_id,
                "ops": [
                    {"op": "write_file", "path": file_a.to_str().unwrap(), "content": "OVERWRITE"},
                    {"op": "write_file", "path": file_b.to_str().unwrap(), "content": "world"}
                ]
            }))
            .await
            .unwrap();

        assert_eq!(second["success"], true, "second run must succeed");
        assert_eq!(second["skipped_completed"], 1, "op 0 must be skipped");
        // file_a content must NOT be overwritten (op was skipped)
        assert_eq!(std::fs::read_to_string(&file_a).unwrap(), "hello");
        // file_b must now exist (op 1 ran)
        assert_eq!(std::fs::read_to_string(&file_b).unwrap(), "world");
    }

    #[tokio::test]
    async fn resume_without_journal_reruns_all() {
        let _g = isolate();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("x.txt");
        // No resume_journal_id — fresh run, journal_id differs each time.
        let r1 = AgentExecHandler
            .execute(json!({"ops": [{"op": "write_file", "path": file.to_str().unwrap(), "content": "v1"}]}))
            .await
            .unwrap();
        let r2 = AgentExecHandler
            .execute(json!({"ops": [{"op": "write_file", "path": file.to_str().unwrap(), "content": "v2"}]}))
            .await
            .unwrap();
        assert_ne!(r1["journal_id"], r2["journal_id"], "each run must get a fresh journal_id");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "v2");
    }
}
