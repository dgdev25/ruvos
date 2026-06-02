//! JSON-RPC MCP server over stdin/stdout.

use crate::{JsonRpcRequest, JsonRpcResponse, Result, RufloError, ToolRegistry};
use tokio::io::{stdin as tokio_stdin, stdout as tokio_stdout};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tracing::info;

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
                RufloError::InternalError(format!("failed to read from stdin: {}", e))
            })?;

            if n == 0 {
                // EOF
                break;
            }

            let response = self.handle_request(&line).await;
            let response_json = serde_json::to_string(&response).map_err(|e| {
                RufloError::InternalError(format!("failed to serialize response: {}", e))
            })?;

            writer
                .write_all(response_json.as_bytes())
                .await
                .map_err(|e| {
                    RufloError::InternalError(format!("failed to write to stdout: {}", e))
                })?;
            writer.write_all(b"\n").await.map_err(|e| {
                RufloError::InternalError(format!("failed to write newline: {}", e))
            })?;
            writer
                .flush()
                .await
                .map_err(|e| RufloError::InternalError(format!("failed to flush stdout: {}", e)))?;
        }

        Ok(())
    }

    async fn handle_request(&self, line: &str) -> JsonRpcResponse {
        match serde_json::from_str::<JsonRpcRequest>(line) {
            Ok(req) => {
                if req.jsonrpc != "2.0" {
                    return JsonRpcResponse::error(
                        req.id,
                        -32600,
                        "jsonrpc must be 2.0".to_string(),
                    );
                }

                match self.registry.execute(&req.method, req.params).await {
                    Ok(result) => JsonRpcResponse::success(req.id, result),
                    Err(err) => {
                        let code = err.json_rpc_code();
                        let message = err.message();
                        JsonRpcResponse::error(req.id, code, message)
                    }
                }
            }
            Err(e) => {
                // Parse error: we can't extract request ID, use placeholder
                JsonRpcResponse::error("unknown".to_string(), -32700, format!("Parse error: {}", e))
            }
        }
    }
}

/// Serve the MCP server on stdin/stdout.
pub async fn serve() -> anyhow::Result<()> {
    info!("MCP server initialized with 20 tool registry");

    let registry = crate::tools::create_registry();
    let server = JsonRpcServer::new(registry);

    server.run().await.map_err(|e| anyhow::anyhow!("{:?}", e))
}
