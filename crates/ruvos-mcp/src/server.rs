//! JSON-RPC MCP server over stdin/stdout.
//!
//! Implements the Model Context Protocol handshake so MCP clients (Claude Code,
//! Codex CLI) can discover and call the 20 rUvOS tools:
//! - `initialize` -> server capabilities + info
//! - `notifications/initialized` -> acknowledged (no response)
//! - `tools/list` -> tool definitions with JSON Schema
//! - `tools/call` -> dispatch to a tool handler by name

use crate::compress_learning::{record_compression_learning, CompressionLearningSignal};
use crate::{paths, JsonRpcRequest, JsonRpcResponse, Result, RuvosError, ToolRegistry};
use compress::defaults::{KEEP_HEAD_LINES, KEEP_TAIL_LINES, MAX_ARRAY_ITEMS, MIN_BYTES};
use compress::{compress_content, CompressionConfig};
use tokio::io::{stdin as tokio_stdin, stdout as tokio_stdout};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tracing::{info, warn};

/// MCP protocol version this server speaks.
const PROTOCOL_VERSION: &str = "2024-11-05";

pub struct JsonRpcServer {
    registry: ToolRegistry,
}

impl JsonRpcServer {
    pub fn new(registry: ToolRegistry) -> Self {
        JsonRpcServer { registry }
    }

    pub async fn run(&self) -> Result<()> {
        let stdin = tokio_stdin();
        let stdout = tokio_stdout();
        let mut reader = BufReader::new(stdin);
        let mut writer = BufWriter::new(stdout);

        let mut line = String::new();

        loop {
            line.clear();
            let n = reader.read_line(&mut line).await.map_err(|e| {
                RuvosError::InternalError(format!("failed to read from stdin: {}", e))
            })?;

            if n == 0 {
                // EOF
                break;
            }

            if line.trim().is_empty() {
                continue;
            }

            // Notifications return None: nothing is written back.
            let Some(response) = self.handle_request(&line).await else {
                continue;
            };

            let response_json = serde_json::to_string(&response).map_err(|e| {
                RuvosError::InternalError(format!("failed to serialize response: {}", e))
            })?;

            writer
                .write_all(response_json.as_bytes())
                .await
                .map_err(|e| {
                    RuvosError::InternalError(format!("failed to write to stdout: {}", e))
                })?;
            writer.write_all(b"\n").await.map_err(|e| {
                RuvosError::InternalError(format!("failed to write newline: {}", e))
            })?;
            writer
                .flush()
                .await
                .map_err(|e| RuvosError::InternalError(format!("failed to flush stdout: {}", e)))?;
        }

        Ok(())
    }

    /// Handle one JSON-RPC line. Returns `None` for notifications (no reply).
    async fn handle_request(&self, line: &str) -> Option<JsonRpcResponse> {
        let req: JsonRpcRequest = match serde_json::from_str(line) {
            Ok(req) => req,
            Err(e) => {
                return Some(JsonRpcResponse::error(
                    None,
                    -32700,
                    format!("Parse error: {}", e),
                ));
            }
        };

        if req.jsonrpc != "2.0" {
            return Some(JsonRpcResponse::error(
                req.id,
                -32600,
                "jsonrpc must be 2.0".to_string(),
            ));
        }

        // Notifications (no id) are fire-and-forget per JSON-RPC / MCP.
        // e.g. "notifications/initialized" — acknowledge silently (no reply).
        req.id.as_ref()?;
        let id = req.id;

        match req.method.as_str() {
            // --- MCP handshake ---
            "initialize" => Some(JsonRpcResponse::success(
                id,
                serde_json::json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "ruvos", "version": env!("CARGO_PKG_VERSION") }
                }),
            )),
            "ping" => Some(JsonRpcResponse::success(id, serde_json::json!({}))),

            // --- Tool discovery ---
            "tools/list" => Some(JsonRpcResponse::success(
                id,
                serde_json::json!({ "tools": self.tool_definitions() }),
            )),

            // --- Tool invocation ---
            "tools/call" => Some(self.handle_tools_call(id, req.params).await),

            // Unknown method
            _ => Some(JsonRpcResponse::error(
                id,
                -32601,
                format!("Method not found: {}", req.method),
            )),
        }
    }

    /// Build MCP tool definitions (name, description, JSON Schema) for tools/list.
    fn tool_definitions(&self) -> Vec<serde_json::Value> {
        self.registry.mcp_tool_definitions()
    }

    /// Dispatch a `tools/call` request to the matching handler.
    async fn handle_tools_call(
        &self,
        id: Option<String>,
        params: serde_json::Value,
    ) -> JsonRpcResponse {
        let name = match params.get("name").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => {
                return JsonRpcResponse::error(
                    id,
                    -32602,
                    "tools/call requires a 'name' parameter".to_string(),
                );
            }
        };

        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        match self.registry.execute(&name, arguments).await {
            Ok(result) => {
                // MCP wraps tool output as content blocks.
                let raw_text =
                    serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
                let compression = if name.starts_with("compress.") {
                    None
                } else {
                    let compressed = compress_content(
                        &raw_text,
                        None,
                        CompressionConfig {
                            min_bytes: MIN_BYTES,
                            keep_head_lines: KEEP_HEAD_LINES,
                            keep_tail_lines: KEEP_TAIL_LINES,
                            max_array_items: MAX_ARRAY_ITEMS,
                        },
                    );
                    if compressed.changed {
                        Some(compressed)
                    } else {
                        None
                    }
                };
                let text = compression
                    .as_ref()
                    .map(|compressed| compressed.compressed.clone())
                    .unwrap_or(raw_text);
                if let Some(compressed) = compression.as_ref() {
                    if let Err(error) =
                        record_compression_learning(&CompressionLearningSignal::from_result(
                            "mcp.tools.call",
                            &name,
                            compressed,
                        ))
                    {
                        warn!("compression learning recording failed for {name}: {error:?}");
                    }
                }
                JsonRpcResponse::success(
                    id,
                    serde_json::json!({
                        "content": [{ "type": "text", "text": text }],
                        "isError": false,
                        "structuredContent": result,
                        "compression": compression.map(|compressed| serde_json::json!({
                            "changed": compressed.changed,
                            "original_bytes": compressed.original_bytes,
                            "compressed_bytes": compressed.compressed_bytes,
                            "bytes_saved": compressed.bytes_saved,
                            "compression_ratio": compressed.compression_ratio,
                            "tokens_before": compressed.tokens_before,
                            "tokens_after": compressed.tokens_after,
                            "original_ref": compressed.original_ref,
                        }))
                    }),
                )
            }
            Err(err) => JsonRpcResponse::error(id, err.json_rpc_code(), err.message()),
        }
    }
}

/// Serve the MCP server on stdin/stdout.
pub async fn serve() -> anyhow::Result<()> {
    paths::ensure_root().map_err(|e| anyhow::anyhow!("initializing data root: {e}"))?;
    info!(
        "MCP server initialized with {} tools",
        crate::tools::tool_registry().len()
    );

    let registry = crate::tools::create_registry();
    let server = JsonRpcServer::new(registry);

    server.run().await.map_err(|e| anyhow::anyhow!("{:?}", e))
}
