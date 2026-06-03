//! Hooks domain tools (3): pre, post, route

use super::handler::{ExecuteFuture, ToolHandler};
use crate::Result;
use ruvos_hooks::{HookDispatcher, HookKind, HookOutcome};
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
// Real implementations for hooks tools
// ============================================================================

pub struct HooksPreHandler {
    dispatcher: HookDispatcher,
}

impl HooksPreHandler {
    pub fn new() -> Self {
        Self {
            dispatcher: HookDispatcher::new(),
        }
    }
}

impl Default for HooksPreHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for HooksPreHandler {
    fn name(&self) -> &'static str {
        "pre"
    }

    fn domain(&self) -> &'static str {
        "hooks"
    }

    fn validate(&self, params: &Value) -> Result<()> {
        // Validate required fields: kind, payload
        if !params.is_object() {
            return Err(crate::rUvOSError::ValidationError(
                "params must be an object".to_string(),
            ));
        }

        let obj = params.as_object().ok_or_else(|| {
            crate::rUvOSError::ValidationError("params must be an object".to_string())
        })?;

        if !obj.contains_key("kind") {
            return Err(crate::rUvOSError::ValidationError(
                "missing required field: kind".to_string(),
            ));
        }

        if !obj.contains_key("payload") {
            return Err(crate::rUvOSError::ValidationError(
                "missing required field: payload".to_string(),
            ));
        }

        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        let dispatcher = self.dispatcher.clone();

        Box::pin(async move {
            let obj = params.as_object().ok_or_else(|| {
                crate::rUvOSError::InvalidParams("params must be an object".to_string())
            })?;

            let kind_str = obj.get("kind").and_then(|v| v.as_str()).ok_or_else(|| {
                crate::rUvOSError::InvalidParams("kind must be a string".to_string())
            })?;

            let hook_kind = match kind_str {
                "task" => HookKind::Task,
                "edit" => HookKind::Edit,
                "command" => HookKind::Command,
                "session" => HookKind::Session,
                _ => {
                    return Err(crate::rUvOSError::InvalidParams(format!(
                        "invalid hook kind: {}",
                        kind_str
                    )))
                }
            };

            let payload = obj
                .get("payload")
                .cloned()
                .unwrap_or(Value::Object(Default::default()));

            let response = dispatcher
                .dispatch_pre(hook_kind, payload)
                .await
                .map_err(|e| crate::rUvOSError::InternalError(e.to_string()))?;

            Ok(json!({
                "status": response.status,
                "routing": response.routing,
                "context": response.context,
            }))
        })
    }
}

pub struct HooksPostHandler {
    dispatcher: HookDispatcher,
}

impl HooksPostHandler {
    pub fn new() -> Self {
        Self {
            dispatcher: HookDispatcher::new(),
        }
    }
}

impl Default for HooksPostHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolHandler for HooksPostHandler {
    fn name(&self) -> &'static str {
        "post"
    }

    fn domain(&self) -> &'static str {
        "hooks"
    }

    fn validate(&self, params: &Value) -> Result<()> {
        // Validate required fields: kind, payload, success, message, metadata
        if !params.is_object() {
            return Err(crate::rUvOSError::ValidationError(
                "params must be an object".to_string(),
            ));
        }

        let obj = params.as_object().ok_or_else(|| {
            crate::rUvOSError::ValidationError("params must be an object".to_string())
        })?;

        if !obj.contains_key("kind") {
            return Err(crate::rUvOSError::ValidationError(
                "missing required field: kind".to_string(),
            ));
        }

        if !obj.contains_key("payload") {
            return Err(crate::rUvOSError::ValidationError(
                "missing required field: payload".to_string(),
            ));
        }

        if !obj.contains_key("success") {
            return Err(crate::rUvOSError::ValidationError(
                "missing required field: success".to_string(),
            ));
        }

        // message and metadata are optional in the request but used in outcome
        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        let dispatcher = self.dispatcher.clone();

        Box::pin(async move {
            let obj = params.as_object().ok_or_else(|| {
                crate::rUvOSError::InvalidParams("params must be an object".to_string())
            })?;

            let kind_str = obj.get("kind").and_then(|v| v.as_str()).ok_or_else(|| {
                crate::rUvOSError::InvalidParams("kind must be a string".to_string())
            })?;

            let hook_kind = match kind_str {
                "task" => HookKind::Task,
                "edit" => HookKind::Edit,
                "command" => HookKind::Command,
                "session" => HookKind::Session,
                _ => {
                    return Err(crate::rUvOSError::InvalidParams(format!(
                        "invalid hook kind: {}",
                        kind_str
                    )))
                }
            };

            let payload = obj
                .get("payload")
                .cloned()
                .unwrap_or(Value::Object(Default::default()));

            let success = obj
                .get("success")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| {
                    crate::rUvOSError::InvalidParams("success must be a boolean".to_string())
                })?;

            let message = obj
                .get("message")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let metadata = obj
                .get("metadata")
                .cloned()
                .unwrap_or(Value::Object(Default::default()));

            let outcome = HookOutcome {
                success,
                message,
                metadata,
            };

            let response = dispatcher
                .dispatch_post(hook_kind, payload, outcome)
                .await
                .map_err(|e| crate::rUvOSError::InternalError(e.to_string()))?;

            Ok(json!({
                "status": response.status,
                "routing": response.routing,
                "context": response.context,
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
