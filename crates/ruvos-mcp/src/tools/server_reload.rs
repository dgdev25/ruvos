//! `ruvos_server_reload` — replace the running MCP server process in-place (ADR-033).
//!
//! Uses execve(2) to atomically replace the current process image with the newly
//! installed binary at the same path and with the same argv.  The MCP session
//! (stdin/stdout) is inherited by the new image so the client sees no disconnect.
//! The call never returns on success; on failure it returns an error JSON.

use crate::tools::handler::{ExecuteFuture, ToolHandler};
use crate::Result;
use serde_json::{json, Value};

pub struct ServerReloadHandler;

impl ToolHandler for ServerReloadHandler {
    fn name(&self) -> &'static str {
        "reload"
    }

    fn domain(&self) -> &'static str {
        "server"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }

    fn execute(&self, _params: Value) -> ExecuteFuture {
        Box::pin(async move { reload_server() })
    }
}

fn reload_server() -> Result<Value> {
    use std::os::unix::process::CommandExt;

    let exe = std::env::current_exe()
        .map_err(|e| crate::RuvosError::InternalError(format!("current_exe: {e}")))?
;
    // Collect all argv[1..] so the new image inherits the same subcommands/flags.
    let args: Vec<String> = std::env::args().skip(1).collect();

    // .exec() replaces this process image; it only returns if execve fails.
    let err = std::process::Command::new(&exe)
        .args(&args)
        .exec();

    Err(crate::RuvosError::InternalError(format!("execve failed: {err}")))
}
