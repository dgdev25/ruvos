//! rmcp-based MCP server implementation (ADR-031).
//!
//! Wraps the existing ToolRegistry with the official modelcontextprotocol/rust-sdk
//! (rmcp v1.7.0) transport layer, replacing the bespoke JSON-RPC stdio loop in
//! server.rs with the SDK's well-tested framing and session management.

use std::sync::Arc;

use rmcp::{
    ServerHandler,
    model::{
        CallToolRequestParams, CallToolResult, Content, Implementation, ListToolsResult,
        ServerCapabilities, ServerInfo, Tool,
    },
    service::{RequestContext, RoleServer},
    serve_server,
};
use rmcp::ErrorData as McpError;
use serde_json::Value;
use tracing::warn;

use crate::{
    compress_learning::{record_compression_learning, CompressionLearningSignal},
    paths,
    tools::create_registry,
    ToolRegistry,
};
use compress::defaults::{KEEP_HEAD_LINES, KEEP_TAIL_LINES, MAX_ARRAY_ITEMS, MIN_BYTES};
use compress::{compress_content, CompressionConfig};

/// Bridges the ruvos ToolRegistry into the rmcp ServerHandler trait.
pub struct RuvosServerHandler {
    registry: ToolRegistry,
}

impl RuvosServerHandler {
    pub fn new(registry: ToolRegistry) -> Self {
        Self { registry }
    }

    /// Convert a single mcp_tool_definitions() JSON entry into an rmcp Tool.
    fn def_to_tool(def: &Value) -> Option<Tool> {
        let name = def["name"].as_str()?.to_owned();
        let description = def["description"].as_str().map(|s| s.to_owned());
        let schema_map = def["inputSchema"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        Some(Tool::new_with_raw(
            name,
            description.map(std::borrow::Cow::Owned),
            Arc::new(schema_map),
        ))
    }
}

impl ServerHandler for RuvosServerHandler {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
        .with_server_info(
            Implementation::new("ruvos", env!("CARGO_PKG_VERSION"))
        )
    }

    fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        let defs = self.registry.mcp_tool_definitions();
        let tools: Vec<Tool> = defs
            .iter()
            .filter_map(Self::def_to_tool)
            .collect();
        std::future::ready(Ok(ListToolsResult::with_all_items(tools)))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        async move {
            let name = request.name.as_ref().to_owned();
            let params = match request.arguments {
                Some(map) => Value::Object(map),
                None => serde_json::json!({}),
            };

            match self.registry.execute(&name, params).await {
                Ok(result) => {
                    let raw_text = serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|_| result.to_string());

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
                        .map(|c| c.compressed.clone())
                        .unwrap_or(raw_text);

                    if let Some(ref compressed) = compression {
                        if let Err(e) = record_compression_learning(
                            &CompressionLearningSignal::from_result(
                                "mcp.tools.call",
                                &name,
                                compressed,
                            ),
                        ) {
                            warn!("compression learning recording failed for {name}: {e:?}");
                        }
                    }

                    let mut cr = CallToolResult::success(vec![Content::text(text)]);
                    cr.structured_content = Some(result);
                    if let Some(c) = compression {
                        let mut meta_map = serde_json::Map::new();
                        meta_map.insert("compression".to_owned(), serde_json::json!({
                            "changed": c.changed,
                            "original_bytes": c.original_bytes,
                            "compressed_bytes": c.compressed_bytes,
                            "bytes_saved": c.bytes_saved,
                            "compression_ratio": c.compression_ratio,
                            "tokens_before": c.tokens_before,
                            "tokens_after": c.tokens_after,
                        }));
                        cr.meta = Some(rmcp::model::Meta(meta_map));
                    }
                    Ok(cr)
                }
                Err(err) => {
                    let msg = err.message();
                    let mut cr = CallToolResult::success(vec![Content::text(msg)]);
                    cr.is_error = Some(true);
                    Ok(cr)
                }
            }
        }
    }
}

/// Start the MCP server on stdin/stdout using the rmcp SDK transport.
pub async fn serve() -> anyhow::Result<()> {
    paths::ensure_root().map_err(|e| anyhow::anyhow!("initializing data root: {e}"))?;
    tracing::info!(
        "ruvos MCP server (rmcp) initializing with {} tools",
        crate::tools::tool_registry().len()
    );
    let registry = create_registry();
    let handler = RuvosServerHandler::new(registry);
    serve_server(handler, (tokio::io::stdin(), tokio::io::stdout()))
        .await
        .map_err(|e| anyhow::anyhow!("rmcp server error: {e}"))?;
    Ok(())
}
