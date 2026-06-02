// crates/ruflo-mcp/tests/integration_test.rs
//! End-to-end integration test for MCP echo round-trip.
//! Validates that the MCP server can be spawned and respond to requests correctly.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::thread;

#[test]
fn test_mcp_echo_round_trip() {
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

    // Note: child is wait()'ed on via kill() below

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

    // Send echo request
    let request = json!({
        "jsonrpc": "2.0",
        "method": "echo.test",
        "params": {"message": "integration-test-123"},
        "id": "test-1"
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
    assert_eq!(
        response["result"]["echo"], "integration-test-123",
        "echo value mismatch"
    );
    assert_eq!(response["id"], "test-1", "request ID mismatch");
    assert!(
        !response["result"]["timestamp"].is_null(),
        "timestamp missing"
    );

    // Clean up
    let _ = child.kill();
    let _ = child.wait();
}
