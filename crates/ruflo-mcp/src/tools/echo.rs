// crates/ruflo-mcp/src/tools/echo.rs
//! Echo tool - a simple test handler for the registry

use super::handler::{ToolHandler, ExecuteFuture};
use crate::Result;
use serde_json::{json, Value};

pub struct EchoHandler;

impl ToolHandler for EchoHandler {
    fn name(&self) -> &'static str {
        "echo"
    }

    fn domain(&self) -> &'static str {
        "test"
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        // Echo accepts any input
        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            Ok(json!({
                "echoed": params,
            }))
        })
    }
}
