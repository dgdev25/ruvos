//! `ruflo mcp serve` command: start the JSON-RPC MCP server on stdio.

use ruflo_mcp::serve as mcp_serve;

/// Serve the MCP server on stdin/stdout.
pub async fn serve() -> anyhow::Result<()> {
    // Initialize and run the MCP server.
    // - Connects to stdin/stdout
    // - Registers all 20 tools (memory, session, agent, hooks, intel, plugin, gov, workflow)
    // - Enters JSON-RPC dispatch loop
    mcp_serve().await
}
