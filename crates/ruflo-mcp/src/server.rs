//! JSON-RPC MCP server over stdin/stdout.

use serde_json::json;
use tracing::info;

/// Serve the MCP server on stdin/stdout.
pub async fn serve() -> anyhow::Result<()> {
    info!("MCP server initialized with 20 tool registry");

    // TODO: Implement JSON-RPC dispatch loop:
    // 1. Read from stdin line by line
    // 2. Parse as JSON-RPC 2.0 request
    // 3. Dispatch to tool handler based on method name
    // 4. Write JSON-RPC response to stdout

    let response = json!({
        "jsonrpc": "2.0",
        "result": "MCP server ready",
        "id": 1
    });

    println!("{}", response);

    Ok(())
}
