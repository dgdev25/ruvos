//! Hooks domain tools (3): pre, post, route

use super::handler::{ExecuteFuture, ToolHandler};
use crate::runtime::{publish_event, RuntimeEvent};
use crate::Result;
use ruvos_hooks::{HookDispatcher, HookKind, HookOutcome};
use ruvos_safety::{SafetyLevel, ValidationRequest};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

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

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "kind": {
                    "type": "string",
                    "enum": ["task", "edit", "command", "session"],
                    "description": "Hook event kind"
                },
                "payload": {
                    "type": "object",
                    "description": "Event payload object",
                    "additionalProperties": true
                }
            },
            "required": ["kind", "payload"]
        })
    }

    fn validate(&self, params: &Value) -> Result<()> {
        // Validate required fields: kind, payload
        if !params.is_object() {
            return Err(crate::RuvosError::ValidationError(
                "params must be an object".to_string(),
            ));
        }

        let obj = params.as_object().ok_or_else(|| {
            crate::RuvosError::ValidationError("params must be an object".to_string())
        })?;

        if !obj.contains_key("kind") {
            return Err(crate::RuvosError::ValidationError(
                "missing required field: kind".to_string(),
            ));
        }

        if !obj.contains_key("payload") {
            return Err(crate::RuvosError::ValidationError(
                "missing required field: payload".to_string(),
            ));
        }

        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        let dispatcher = self.dispatcher.clone();

        Box::pin(async move {
            let obj = params.as_object().ok_or_else(|| {
                crate::RuvosError::InvalidParams("params must be an object".to_string())
            })?;

            let kind_str = obj.get("kind").and_then(|v| v.as_str()).ok_or_else(|| {
                crate::RuvosError::InvalidParams("kind must be a string".to_string())
            })?;

            let hook_kind = match kind_str {
                "task" => HookKind::Task,
                "edit" => HookKind::Edit,
                "command" => HookKind::Command,
                "session" => HookKind::Session,
                _ => {
                    return Err(crate::RuvosError::InvalidParams(format!(
                        "invalid hook kind: {}",
                        kind_str
                    )))
                }
            };

            let payload = obj
                .get("payload")
                .cloned()
                .unwrap_or(Value::Object(Default::default()));

            publish_event(RuntimeEvent {
                kind: "hooks.pre.started".to_string(),
                payload: json!({
                    "kind": kind_str,
                    "payload_keys": payload.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default(),
                }),
                agent_id: None,
                task_id: None,
            });

            let response = dispatcher
                .dispatch_pre(hook_kind, payload.clone())
                .await
                .map_err(|e| crate::RuvosError::InternalError(e.to_string()))?;
            let response_status = response.status.clone();
            let response_routing = response.routing.clone();
            let response_context = response.context.clone();

            let mut out = json!({
                "status": response_status,
                "routing": response_routing,
                "context": response_context,
            });

            // Additive risk assessment for edit / command pre-hooks: run the
            // payload content through the shared SafetyEngine.
            if matches!(hook_kind, HookKind::Edit | HookKind::Command) {
                let (safety, blocked) = Self::assess_risk(hook_kind, &payload);
                if let Value::Object(ref mut map) = out {
                    map.insert("safety".to_string(), safety);
                    map.insert("blocked".to_string(), json!(blocked));
                }
            }

            // ADR-034: AISP prompt-precision layer for task pre-hooks. Off by
            // default; when enabled in ~/.ruvos/hooks.json it converts the task
            // prose to AISP, validates the tier, and (if a min_tier gate is set
            // and warn_only=false) can mark the task blocked.
            if matches!(hook_kind, HookKind::Task) {
                let cfg = crate::tools::aisp_layer::AispConfig::load();
                if cfg.enabled {
                    let prose = Self::extract_task_text(&payload);
                    if !prose.trim().is_empty() {
                        let assessment = crate::tools::aisp_layer::assess(&prose, &cfg);
                        let blocked = assessment.blocked;
                        if let Value::Object(ref mut map) = out {
                            map.insert("aisp".to_string(), assessment.to_json());
                            if blocked {
                                map.insert("status".to_string(), json!("blocked"));
                            }
                        }
                    }
                }
            }

            publish_event(RuntimeEvent {
                kind: "hooks.pre.completed".to_string(),
                payload: json!({
                    "kind": kind_str,
                    "status": response.status,
                    "routing": response.routing,
                }),
                agent_id: None,
                task_id: None,
            });

            Ok(out)
        })
    }
}

impl HooksPreHandler {
    /// Extract task-spec prose from a task hook payload, preferring fields that
    /// carry the natural-language objective. Falls back to `extract_content`.
    fn extract_task_text(payload: &Value) -> String {
        for key in ["prompt", "task", "description", "objective", "spec"] {
            if let Some(s) = payload.get(key).and_then(|v| v.as_str()) {
                if !s.trim().is_empty() {
                    return s.to_string();
                }
            }
        }
        Self::extract_content(payload)
    }

    /// Extract the most relevant text to scan from a hook payload, preferring
    /// well-known fields and falling back to the stringified payload.
    fn extract_content(payload: &Value) -> String {
        for key in ["content", "command", "cmd", "text", "code", "diff"] {
            if let Some(s) = payload.get(key).and_then(|v| v.as_str()) {
                return s.to_string();
            }
        }
        match payload {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        }
    }

    /// Validate the payload through the process-global SafetyEngine and return
    /// `(safety_json, blocked)`. `blocked` is true when a Critical or High
    /// violation is detected, signalling the action is risky.
    fn assess_risk(kind: HookKind, payload: &Value) -> (Value, bool) {
        let action = match kind {
            HookKind::Edit => "file_write",
            HookKind::Command => "command",
            _ => "unknown",
        };
        let mut context = HashMap::new();
        context.insert("action".to_string(), action.to_string());

        let request = ValidationRequest {
            content: Self::extract_content(payload),
            context,
            safety_level: SafetyLevel::Medium,
        };

        let engine = crate::safety::engine();
        let resp = {
            let guard = engine.lock().unwrap_or_else(|p| p.into_inner());
            guard.validate(&request)
        };

        let blocked = resp.violations.iter().any(|v| v.level >= SafetyLevel::High);

        let violations: Vec<Value> = resp
            .violations
            .iter()
            .map(|v| {
                json!({
                    "constraint": v.constraint_name,
                    "level": v.level.to_string(),
                    "message": v.message,
                })
            })
            .collect();

        let safety = json!({
            "passed": resp.passed,
            "safety_score": resp.safety_score,
            "violations": violations,
        });

        (safety, blocked)
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

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "kind": {
                    "type": "string",
                    "enum": ["task", "edit", "command", "session"],
                    "description": "Hook event kind"
                },
                "payload": {
                    "type": "object",
                    "description": "Event payload object",
                    "additionalProperties": true
                },
                "success": {
                    "type": "boolean",
                    "description": "Whether the hooked operation succeeded"
                },
                "message": {
                    "type": "string",
                    "description": "Optional outcome message"
                },
                "metadata": {
                    "type": "object",
                    "description": "Optional additional metadata",
                    "additionalProperties": true
                }
            },
            "required": ["kind", "payload", "success"]
        })
    }

    fn validate(&self, params: &Value) -> Result<()> {
        // Validate required fields: kind, payload, success, message, metadata
        if !params.is_object() {
            return Err(crate::RuvosError::ValidationError(
                "params must be an object".to_string(),
            ));
        }

        let obj = params.as_object().ok_or_else(|| {
            crate::RuvosError::ValidationError("params must be an object".to_string())
        })?;

        if !obj.contains_key("kind") {
            return Err(crate::RuvosError::ValidationError(
                "missing required field: kind".to_string(),
            ));
        }

        if !obj.contains_key("payload") {
            return Err(crate::RuvosError::ValidationError(
                "missing required field: payload".to_string(),
            ));
        }

        if !obj.contains_key("success") {
            return Err(crate::RuvosError::ValidationError(
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
                crate::RuvosError::InvalidParams("params must be an object".to_string())
            })?;

            let kind_str = obj.get("kind").and_then(|v| v.as_str()).ok_or_else(|| {
                crate::RuvosError::InvalidParams("kind must be a string".to_string())
            })?;

            let hook_kind = match kind_str {
                "task" => HookKind::Task,
                "edit" => HookKind::Edit,
                "command" => HookKind::Command,
                "session" => HookKind::Session,
                _ => {
                    return Err(crate::RuvosError::InvalidParams(format!(
                        "invalid hook kind: {}",
                        kind_str
                    )))
                }
            };

            let payload = obj
                .get("payload")
                .cloned()
                .unwrap_or(Value::Object(Default::default()));

            publish_event(RuntimeEvent {
                kind: "hooks.post.started".to_string(),
                payload: json!({
                    "kind": kind_str,
                    "success": obj.get("success").and_then(|v| v.as_bool()).unwrap_or(false),
                }),
                agent_id: None,
                task_id: None,
            });

            let success = match obj.get("success") {
                Some(Value::Bool(b)) => *b,
                Some(Value::String(s)) => s == "true",
                _ => {
                    return Err(crate::RuvosError::InvalidParams(
                        "success must be a boolean (or string \"true\"/\"false\")".to_string(),
                    ))
                }
            };

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
                .map_err(|e| crate::RuvosError::InternalError(e.to_string()))?;
            let response_status = response.status.clone();
            let response_routing = response.routing.clone();
            let response_context = response.context.clone();

            publish_event(RuntimeEvent {
                kind: "hooks.post.completed".to_string(),
                payload: json!({
                    "kind": kind_str,
                    "status": response_status.clone(),
                    "routing": response_routing.clone(),
                }),
                agent_id: None,
                task_id: None,
            });

            Ok(json!({
                "status": response_status,
                "routing": response_routing,
                "context": response_context,
            }))
        })
    }
}

pub use super::hooks_route::HooksRouteHandler;

#[cfg(test)]
mod safety_tests {
    use super::*;

    fn isolate() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(dir.path().to_path_buf());
        dir
    }

    #[tokio::test]
    async fn pre_command_flags_destructive_command() {
        let _g = isolate();
        let r = HooksPreHandler::new()
            .execute(json!({
                "kind": "command",
                "payload": { "command": "rm -rf /var/data" }
            }))
            .await
            .unwrap();

        // A dangerous pattern must be surfaced and the action blocked.
        assert_eq!(r["blocked"], true, "destructive command must be blocked");
        let violations = r["safety"]["violations"].as_array().unwrap();
        assert!(!violations.is_empty(), "expected at least one violation");
        assert!(r["safety"]["passed"] == false);
    }

    #[tokio::test]
    async fn pre_command_allows_safe_command() {
        let _g = isolate();
        let r = HooksPreHandler::new()
            .execute(json!({
                "kind": "command",
                "payload": { "command": "ls -la" }
            }))
            .await
            .unwrap();

        assert_eq!(r["blocked"], false, "safe command must not be blocked");
        assert_eq!(r["safety"]["passed"], true);
    }

    #[tokio::test]
    async fn post_accepts_boolean_string_true() {
        let _g = isolate();
        let r = HooksPostHandler::new()
            .execute(serde_json::json!({
                "kind": "task",
                "payload": {},
                "success": "true"
            }))
            .await
            .unwrap();
        assert_eq!(r["status"], "ok");
    }

    #[tokio::test]
    async fn post_accepts_boolean_string_false() {
        let _g = isolate();
        let r = HooksPostHandler::new()
            .execute(serde_json::json!({
                "kind": "task",
                "payload": {},
                "success": "false"
            }))
            .await
            .unwrap();
        assert_eq!(r["status"], "ok");
    }

    #[tokio::test]
    async fn pre_task_aisp_absent_when_disabled() {
        let _g = isolate();
        // No hooks.json written → AISP layer is disabled by default.
        let r = HooksPreHandler::new()
            .execute(json!({
                "kind": "task",
                "payload": { "prompt": "For all users, if admin then allow access" }
            }))
            .await
            .unwrap();
        assert!(
            r.get("aisp").is_none(),
            "AISP must be absent unless enabled"
        );
    }

    #[tokio::test]
    async fn pre_task_aisp_attached_when_enabled() {
        let g = isolate();
        // Enable the AISP layer via ~/.ruvos/hooks.json in the isolated root,
        // warn_only so it never blocks.
        let cfg_path = crate::paths::data_root().join("hooks.json");
        std::fs::create_dir_all(cfg_path.parent().unwrap()).unwrap();
        std::fs::write(
            &cfg_path,
            r#"{"aisp":{"enabled":true,"warn_only":true,"auto_convert":true}}"#,
        )
        .unwrap();

        let r = HooksPreHandler::new()
            .execute(json!({
                "kind": "task",
                "payload": { "prompt": "For all users, if admin then allow access" }
            }))
            .await
            .unwrap();

        let aisp = r
            .get("aisp")
            .expect("AISP assessment must be attached when enabled");
        assert!(aisp["tier"].is_string());
        assert!(aisp["aisp_spec"].is_string());
        assert_eq!(aisp["blocked"], false, "warn_only must not block");
        // status must remain the dispatcher's value (not 'blocked') under warn_only.
        assert_ne!(r["status"], "blocked");
        drop(g);
    }

    #[tokio::test]
    async fn pre_task_aisp_blocks_below_min_tier() {
        let g = isolate();
        let cfg_path = crate::paths::data_root().join("hooks.json");
        std::fs::create_dir_all(cfg_path.parent().unwrap()).unwrap();
        // Hard gate at Platinum, warn_only off → vague prose must block.
        std::fs::write(
            &cfg_path,
            r#"{"aisp":{"enabled":true,"min_tier":"platinum","warn_only":false}}"#,
        )
        .unwrap();

        let r = HooksPreHandler::new()
            .execute(json!({
                "kind": "task",
                "payload": { "prompt": "do the thing somehow" }
            }))
            .await
            .unwrap();

        assert_eq!(r["aisp"]["blocked"], true);
        assert_eq!(r["status"], "blocked");
        drop(g);
    }

    #[tokio::test]
    async fn pre_task_has_no_safety_field() {
        let _g = isolate();
        let r = HooksPreHandler::new()
            .execute(json!({
                "kind": "task",
                "payload": { "content": "rm -rf /tmp" }
            }))
            .await
            .unwrap();

        // Risk assessment is only wired for edit/command; task is untouched.
        assert!(r.get("safety").is_none());
        assert!(r.get("blocked").is_none());
    }
}
