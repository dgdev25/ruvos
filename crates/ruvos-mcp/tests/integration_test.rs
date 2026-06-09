// crates/ruvos-mcp/tests/integration_test.rs
//! End-to-end integration test for the MCP protocol handshake.
//! Validates that the rUvOS MCP server speaks real MCP: initialize, tools/list,
//! and tools/call — the exact sequence an MCP client (Claude Code) performs.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread;

/// Read the next JSON-RPC response line (skipping any tracing output on stdout).
fn read_response(reader: &mut impl BufRead) -> Value {
    let mut line = String::new();
    loop {
        line.clear();
        reader
            .read_line(&mut line)
            .expect("failed to read response");
        if line.trim().is_empty() {
            panic!("reached EOF without finding JSON response");
        }
        if line.trim().starts_with('{') {
            return serde_json::from_str(&line).expect("failed to parse response JSON");
        }
    }
}

fn send(stdin: &mut ChildStdin, req: &Value) {
    stdin
        .write_all(format!("{}\n", req).as_bytes())
        .expect("failed to write request");
}

fn spawn_server() -> Child {
    let build = Command::new("cargo")
        .args(["build", "--release", "-p", "ruvos-cli"])
        .output()
        .expect("failed to build ruvos-cli");
    assert!(
        build.status.success(),
        "cargo build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let binary_path = format!("{}/../../target/release/ruvos", manifest_dir);

    Command::new(&binary_path)
        .args(["mcp", "serve"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ruvos mcp serve")
}

#[test]
fn test_mcp_protocol_handshake() {
    let mut child = spawn_server();

    let mut stdin = child.stdin.take().expect("failed to get stdin");
    let stdout = child.stdout.take().expect("failed to get stdout");
    let stderr = child.stderr.take().expect("failed to get stderr");
    let mut reader = BufReader::new(stdout);

    // Drain stderr so tracing output doesn't block the process.
    thread::spawn(move || {
        let mut stderr_reader = BufReader::new(stderr);
        let mut line = String::new();
        while stderr_reader.read_line(&mut line).is_ok() && !line.is_empty() {
            line.clear();
        }
    });

    // 1. initialize — the client's first message.
    send(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": {"protocolVersion": "2024-11-05", "capabilities": {},
                       "clientInfo": {"name": "test", "version": "1"}}
        }),
    );
    let init = read_response(&mut reader);
    assert_eq!(init["result"]["serverInfo"]["name"], "ruvos");
    assert_eq!(init["result"]["protocolVersion"], "2024-11-05");

    // 2. initialized notification — no response expected.
    send(
        &mut stdin,
        &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    );

    // 3. tools/list — must return objects with name + inputSchema, not bare strings.
    send(
        &mut stdin,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}),
    );
    let list = read_response(&mut reader);
    let tools = list["result"]["tools"].as_array().expect("tools array");
    assert_eq!(
        tools.len(),
        ruvos_mcp::tools::public_tool_count(),
        "expected all public rUvOS tools"
    );
    assert!(tools.iter().any(|t| t["name"] == "ruvos_session_create"));
    assert!(
        tools.iter().all(|t| t["inputSchema"].is_object()),
        "every tool must expose an inputSchema"
    );

    // 4. tools/call — dispatch a real tool and verify MCP content envelope.
    send(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0", "id": 2, "method": "tools/call",
            "params": {"name": "ruvos_session_create", "arguments": {"name": "itest"}}
        }),
    );
    let call = read_response(&mut reader);
    assert!(
        call["error"].is_null(),
        "tools/call errored: {}",
        call["error"]
    );
    assert_eq!(call["result"]["isError"], false);
    assert_eq!(call["result"]["content"][0]["type"], "text");
    assert_eq!(call["result"]["structuredContent"]["status"], "created");
    assert!(
        call["result"]["structuredContent"]["session_id"].is_string(),
        "ruvos_session_create must return a session_id"
    );

    // 5. Seed a deterministic large payload in memory, then list it back and
    // verify the MCP response includes a compression envelope.
    let namespace = uuid::Uuid::new_v4().to_string();
    for i in 0..30 {
        let key = format!("key-{i}");
        let value = format!("payload-{i}-{}", "x".repeat(256));
        send(
            &mut stdin,
            &json!({
                "jsonrpc": "2.0", "id": 100 + i, "method": "tools/call",
                "params": {
                    "name": "ruvos_memory_store",
                    "arguments": {
                        "namespace": namespace.clone(),
                        "key": key,
                        "value": value,
                    }
                }
            }),
        );
        let stored = read_response(&mut reader);
        assert!(
            stored["error"].is_null(),
            "ruvos_memory_store errored: {}",
            stored["error"]
        );
    }
    send(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": {
                "name": "ruvos_memory_list",
                "arguments": { "namespace": namespace }
            }
        }),
    );
    let compressed = read_response(&mut reader);
    assert!(
        compressed["error"].is_null(),
        "memory.list errored: {}",
        compressed["error"]
    );
    assert_eq!(compressed["result"]["isError"], false);
    // rmcp places compression metadata in _meta (MCP protocol field), not a top-level key.
    assert!(
        compressed["result"]["_meta"]["compression"]["changed"]
            .as_bool()
            .unwrap_or(false),
        "large MCP tool outputs should be compressed (check _meta.compression)"
    );

    drop(stdin);
    let _ = child.kill();
    let _ = child.wait();
}
