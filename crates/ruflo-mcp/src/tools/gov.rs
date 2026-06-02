//! Gov domain tools (2): witness_verify, health

use super::handler::{ExecuteFuture, ToolHandler};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub substrate: String,
    pub hosts: String,
    pub mcp: String,
}

// ============================================================================
// Stub handlers for gov tools
// ============================================================================

pub struct GovWitnessVerifyStub;

impl ToolHandler for GovWitnessVerifyStub {
    fn name(&self) -> &'static str {
        "witness_verify"
    }

    fn domain(&self) -> &'static str {
        "gov"
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        // TODO: Validate required field: rvf_path
        Ok(())
    }

    fn execute(&self, _params: Value) -> ExecuteFuture {
        Box::pin(async move {
            // TODO: Invoke rvf-crypto to verify signature chain
            Ok(json!({
                "verified": true,
            }))
        })
    }
}

pub struct GovHealthStub;

impl ToolHandler for GovHealthStub {
    fn name(&self) -> &'static str {
        "health"
    }

    fn domain(&self) -> &'static str {
        "gov"
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }

    fn execute(&self, _params: Value) -> ExecuteFuture {
        Box::pin(async move {
            // TODO: Query all subsystems and aggregate health
            Ok(json!({
                "substrate": "ok",
                "hosts": "ok",
                "mcp": "ok",
            }))
        })
    }
}
