// crates/ruflo-mcp/tests/plugin_integration_test.rs
//! End-to-end integration tests for plugin system (plugin.list and plugin.invoke).
//! Validates that the MCP server can discover plugins and invoke commands end-to-end.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::thread;

/// Test plugin.list integration end-to-end
/// Spawns the MCP server and validates plugin discovery response structure
#[tokio::test]
#[ignore]
async fn test_plugin_list_integration() {
    // Build the binary first
    let build = Command::new("cargo")
        .args(["build", "--release", "-p", "ruflo-cli"])
        .output()
        .expect("failed to build ruflo-cli");

    if !build.status.success() {
        panic!(
            "cargo build failed: {}",
            String::from_utf8_lossy(&build.stderr)
        );
    }

    // Spawn the MCP server
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let binary_path = format!("{}/../../target/release/ruflo-cli", manifest_dir);

    let mut child = Command::new(&binary_path)
        .args(["mcp", "serve"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ruflo mcp serve");

    let mut stdin = child.stdin.take().expect("failed to get stdin");
    let stdout = child.stdout.take().expect("failed to get stdout");
    let stderr = child.stderr.take().expect("failed to get stderr");
    let mut reader = BufReader::new(stdout);

    // Spawn a thread to consume stderr so it doesn't block the process
    thread::spawn(move || {
        let mut stderr_reader = BufReader::new(stderr);
        let mut line = String::new();
        while stderr_reader.read_line(&mut line).is_ok() && !line.is_empty() {
            line.clear();
        }
    });

    // Send plugin.list request
    let request = json!({
        "jsonrpc": "2.0",
        "method": "plugin.list",
        "params": {},
        "id": "plugin-list-1"
    });

    let request_str = format!("{}\n", request);
    stdin
        .write_all(request_str.as_bytes())
        .expect("failed to write request");
    drop(stdin); // Close stdin so server knows we're done

    // Read response lines until we find a JSON response
    let mut response_line = String::new();
    loop {
        response_line.clear();
        reader
            .read_line(&mut response_line)
            .expect("failed to read response");

        if response_line.trim().is_empty() {
            panic!("reached EOF without finding JSON response");
        }

        // Skip non-JSON lines (like tracing output that went to stdout)
        if response_line.trim().starts_with('{') {
            break;
        }
    }

    // Parse and verify response
    let response: Value =
        serde_json::from_str(&response_line).expect("failed to parse response JSON");

    // Assertions
    assert_eq!(response["jsonrpc"], "2.0", "jsonrpc version mismatch");
    assert!(
        response["error"].is_null(),
        "response contains error: {}",
        response["error"]
    );
    assert!(
        !response["result"].is_null(),
        "response missing result field"
    );
    assert!(
        !response["result"]["count"].is_null(),
        "result missing count field"
    );
    assert_eq!(response["id"], "plugin-list-1", "request ID mismatch");

    // Verify result is a valid object with plugins array and count
    let result = &response["result"];
    assert!(
        result["plugins"].is_array(),
        "plugins field must be an array"
    );
    assert!(result["count"].is_number(), "count field must be a number");

    // Clean up
    let _ = child.kill();
    let _ = child.wait();
}

/// Test plugin.invoke integration end-to-end
/// Spawns the MCP server and validates plugin invocation response structure
#[tokio::test]
#[ignore]
async fn test_plugin_invoke_integration() {
    // Build the binary first
    let build = Command::new("cargo")
        .args(["build", "--release", "-p", "ruflo-cli"])
        .output()
        .expect("failed to build ruflo-cli");

    if !build.status.success() {
        panic!(
            "cargo build failed: {}",
            String::from_utf8_lossy(&build.stderr)
        );
    }

    // Spawn the MCP server
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let binary_path = format!("{}/../../target/release/ruflo-cli", manifest_dir);

    let mut child = Command::new(&binary_path)
        .args(["mcp", "serve"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ruflo mcp serve");

    let mut stdin = child.stdin.take().expect("failed to get stdin");
    let stdout = child.stdout.take().expect("failed to get stdout");
    let stderr = child.stderr.take().expect("failed to get stderr");
    let mut reader = BufReader::new(stdout);

    // Spawn a thread to consume stderr so it doesn't block the process
    thread::spawn(move || {
        let mut stderr_reader = BufReader::new(stderr);
        let mut line = String::new();
        while stderr_reader.read_line(&mut line).is_ok() && !line.is_empty() {
            line.clear();
        }
    });

    // Send plugin.invoke request with echo command
    let request = json!({
        "jsonrpc": "2.0",
        "method": "plugin.invoke",
        "params": {
            "plugin_name": "test-plugin",
            "command": "echo",
            "args": ["hello", "world"]
        },
        "id": "plugin-invoke-1"
    });

    let request_str = format!("{}\n", request);
    stdin
        .write_all(request_str.as_bytes())
        .expect("failed to write request");
    drop(stdin); // Close stdin so server knows we're done

    // Read response lines until we find a JSON response
    let mut response_line = String::new();
    loop {
        response_line.clear();
        reader
            .read_line(&mut response_line)
            .expect("failed to read response");

        if response_line.trim().is_empty() {
            panic!("reached EOF without finding JSON response");
        }

        // Skip non-JSON lines (like tracing output that went to stdout)
        if response_line.trim().starts_with('{') {
            break;
        }
    }

    // Parse and verify response
    let response: Value =
        serde_json::from_str(&response_line).expect("failed to parse response JSON");

    // Assertions
    assert_eq!(response["jsonrpc"], "2.0", "jsonrpc version mismatch");
    assert_eq!(response["id"], "plugin-invoke-1", "request ID mismatch");

    // Verify result fields
    let result = &response["result"];
    assert!(!result["status"].is_null(), "result missing status field");
    assert!(!result["stdout"].is_null(), "result missing stdout field");
    assert!(!result["stderr"].is_null(), "result missing stderr field");

    // Verify field types
    assert!(result["status"].is_number(), "status must be a number");
    assert!(result["stdout"].is_string(), "stdout must be a string");
    assert!(result["stderr"].is_string(), "stderr must be a string");

    // Clean up
    let _ = child.kill();
    let _ = child.wait();
}
