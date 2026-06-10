// crates/ruvos-mcp/tests/plugin_integration_test.rs
//! End-to-end integration tests for plugin system (plugin.list and plugin.invoke).
//! Validates that the MCP server can discover plugins and invoke commands end-to-end.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread;

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

fn do_handshake(stdin: &mut ChildStdin, reader: &mut impl BufRead) {
    send(
        stdin,
        &json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": {"protocolVersion": "2024-11-05", "capabilities": {},
                       "clientInfo": {"name": "test", "version": "1"}}
        }),
    );
    let init = read_response(reader);
    assert_eq!(
        init["result"]["serverInfo"]["name"], "ruvos",
        "initialize response must identify server as ruvos"
    );
    send(
        stdin,
        &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    );
}

/// Test plugin.list integration end-to-end
/// Spawns the MCP server and validates plugin discovery response structure
#[test]
fn test_plugin_list_integration() {
    let mut child = spawn_server();

    let mut stdin = child.stdin.take().expect("failed to get stdin");
    let stdout = child.stdout.take().expect("failed to get stdout");
    let stderr = child.stderr.take().expect("failed to get stderr");
    let mut reader = BufReader::new(stdout);

    thread::spawn(move || {
        let mut stderr_reader = BufReader::new(stderr);
        let mut line = String::new();
        while stderr_reader.read_line(&mut line).is_ok() && !line.is_empty() {
            line.clear();
        }
    });

    do_handshake(&mut stdin, &mut reader);

    send(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": { "name": "ruvos_plugin_list", "arguments": {} },
            "id": "plugin-list-1"
        }),
    );
    drop(stdin);

    let response = read_response(&mut reader);

    assert_eq!(response["jsonrpc"], "2.0", "jsonrpc version mismatch");
    assert!(
        response["error"].is_null(),
        "response contains error: {}",
        response["error"]
    );
    assert_eq!(response["id"], "plugin-list-1", "request ID mismatch");
    assert_eq!(response["result"]["isError"], false, "tool reported error");

    let result = &response["result"]["structuredContent"];
    assert!(
        result["plugins"].is_array(),
        "plugins field must be an array"
    );
    assert!(result["count"].is_number(), "count field must be a number");

    let _ = child.kill();
    let _ = child.wait();
}

/// Test plugin.invoke integration end-to-end
/// Spawns the MCP server and validates plugin invocation response structure
#[test]
fn test_plugin_invoke_integration() {
    let mut child = spawn_server();

    let mut stdin = child.stdin.take().expect("failed to get stdin");
    let stdout = child.stdout.take().expect("failed to get stdout");
    let stderr = child.stderr.take().expect("failed to get stderr");
    let mut reader = BufReader::new(stdout);

    thread::spawn(move || {
        let mut stderr_reader = BufReader::new(stderr);
        let mut line = String::new();
        while stderr_reader.read_line(&mut line).is_ok() && !line.is_empty() {
            line.clear();
        }
    });

    do_handshake(&mut stdin, &mut reader);

    // Invoking a plugin that does not exist — the security layer must reject it.
    send(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": "ruvos_plugin_invoke",
                "arguments": {
                    "plugin_name": "test-plugin",
                    "command": "echo",
                    "args": ["hello", "world"]
                }
            },
            "id": "plugin-invoke-1"
        }),
    );
    drop(stdin);

    let response = read_response(&mut reader);

    assert_eq!(response["jsonrpc"], "2.0", "jsonrpc version mismatch");
    assert_eq!(response["id"], "plugin-invoke-1", "request ID mismatch");

    let result = &response["result"]["structuredContent"];
    assert_eq!(
        result["status"], 1,
        "unknown plugin must yield non-zero status"
    );
    assert_eq!(
        result["stdout"], "",
        "rejected command must not produce stdout"
    );
    assert!(
        result["stderr"]
            .as_str()
            .unwrap_or_default()
            .contains("not found"),
        "stderr must explain the rejection, got: {}",
        result["stderr"]
    );

    let _ = child.kill();
    let _ = child.wait();
}
