//! Hooks domain tools (3): pre, post, route

use super::handler::{ToolHandler, ExecuteFuture};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookPayload {
    pub kind: String,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRecommendation {
    pub model: String,
    pub archetype: String,
    pub confidence: f32,
}

// ============================================================================
// Stub handlers for hooks tools
// ============================================================================

pub struct HooksPreStub;

impl ToolHandler for HooksPreStub {
    fn name(&self) -> &'static str {
        "pre"
    }

    fn domain(&self) -> &'static str {
        "hooks"
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        // TODO: Validate required fields: kind, data
        Ok(())
    }

    fn execute(&self, _params: Value) -> ExecuteFuture {
        Box::pin(async move {
            // TODO: Dispatch to ruflo-hooks, invoke pre-hook logic
            Ok(json!({
                "model": "",
                "archetype": "",
                "confidence": 0.0,
            }))
        })
    }
}

pub struct HooksPostStub;

impl ToolHandler for HooksPostStub {
    fn name(&self) -> &'static str {
        "post"
    }

    fn domain(&self) -> &'static str {
        "hooks"
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        // TODO: Validate required fields: kind, data, outcome
        Ok(())
    }

    fn execute(&self, _params: Value) -> ExecuteFuture {
        Box::pin(async move {
            // TODO: Dispatch to ruflo-hooks, invoke post-hook logic, feed to sona
            Ok(json!({
                "status": "processed",
            }))
        })
    }
}

pub struct HooksRouteStub;

impl ToolHandler for HooksRouteStub {
    fn name(&self) -> &'static str {
        "route"
    }

    fn domain(&self) -> &'static str {
        "hooks"
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        // TODO: Validate required field: task
        Ok(())
    }

    fn execute(&self, _params: Value) -> ExecuteFuture {
        Box::pin(async move {
            // TODO: Query ruvector-router-core
            Ok(json!({
                "model": "",
                "archetype": "",
                "confidence": 0.0,
            }))
        })
    }
}
