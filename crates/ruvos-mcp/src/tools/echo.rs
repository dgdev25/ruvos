// crates/ruvos-mcp/src/tools/echo.rs
//! Echo tool - a simple test handler for the registry

use super::handler::{ExecuteFuture, ToolHandler};
use crate::Result;
use chrono::Utc;
use serde_json::{json, Value};

pub struct EchoHandler;

impl ToolHandler for EchoHandler {
    fn name(&self) -> &'static str {
        "test"
    }

    fn domain(&self) -> &'static str {
        "echo"
    }

    fn validate(&self, params: &Value) -> Result<()> {
        if !params.is_object() {
            return Err(crate::RuvosError::InvalidParams(
                "params must be an object".to_string(),
            ));
        }

        if params.get("message").and_then(|v| v.as_str()).is_none() {
            return Err(crate::RuvosError::InvalidParams(
                "missing 'message' field (string)".to_string(),
            ));
        }

        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let message = params
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap()
                .to_string();

            Ok(json!({
                "echo": message,
                "timestamp": Utc::now().to_rfc3339(),
                "handler": "echo",
            }))
        })
    }
}
