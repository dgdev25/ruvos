//! `ruflo mcp serve` command: start the JSON-RPC MCP server on stdio.

use tracing::info;

/// Serve the MCP server on stdin/stdout.
pub async fn serve() -> anyhow::Result<()> {
    info!("MCP server starting on stdio");

    // TODO: Initialize and run the MCP server.
    // - Connect to stdin/stdout
    // - Register all 20 tools (memory, session, agent, hooks, intel, plugin, gov, workflow)
    // - Enter JSON-RPC dispatch loop

    Ok(())
}
