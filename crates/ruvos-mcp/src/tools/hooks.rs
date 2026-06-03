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

            let response = dispatcher
                .dispatch_pre(hook_kind, payload)
                .await
                .map_err(|e| crate::RuvosError::InternalError(e.to_string()))?;

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

            let success = obj
                .get("success")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| {
                    crate::RuvosError::InvalidParams("success must be a boolean".to_string())
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
                .map_err(|e| crate::RuvosError::InternalError(e.to_string()))?;

            Ok(json!({
                "status": response.status,
                "routing": response.routing,
                "context": response.context,
            }))
        })
    }
}

/// Routes a task to the best archetype + model tier using real keyword
/// heuristics over the task text. Returns a confidence based on how strongly
/// the task matched a known archetype's signal words.
pub struct HooksRouteHandler;

impl HooksRouteHandler {
    /// (archetype, signal keywords, model tier).
    const RULES: &'static [(&'static str, &'static [&'static str], u32)] = &[
        ("tester", &["test", "spec", "tdd", "coverage", "assert"], 2),
        (
            "security",
            &["security", "vuln", "auth", "exploit", "threat", "injection"],
            3,
        ),
        (
            "perf",
            &[
                "perf",
                "performance",
                "optimize",
                "latency",
                "benchmark",
                "profil",
            ],
            3,
        ),
        (
            "reviewer",
            &["review", "lint", "quality", "style", "refactor"],
            3,
        ),
        (
            "architect",
            &["architect", "design", "interface", "boundary", "system"],
            3,
        ),
        (
            "planner",
            &["plan", "decompose", "roadmap", "milestone", "breakdown"],
            3,
        ),
        (
            "devops",
            &[
                "deploy",
                "ci",
                "cd",
                "pipeline",
                "docker",
                "kubernetes",
                "infra",
            ],
            2,
        ),
        (
            "data",
            &["schema", "migration", "database", "sql", "query", "table"],
            2,
        ),
        (
            "docs",
            &["document", "docs", "readme", "guide", "tutorial"],
            2,
        ),
        (
            "researcher",
            &["research", "investigate", "explore", "find", "discover"],
            2,
        ),
        (
            "coordinator",
            &[
                "coordinate",
                "orchestrate",
                "swarm",
                "multi-agent",
                "delegate",
            ],
            3,
        ),
        (
            "coder",
            &["implement", "build", "code", "endpoint", "function", "fix"],
            2,
        ),
    ];

    fn model_for_tier(tier: u32) -> &'static str {
        match tier {
            3 => "claude-opus-4-8",
            2 => "claude-haiku-4-5",
            _ => "claude-haiku-4-5",
        }
    }

    fn route(task: &str) -> (String, &'static str, u32, f64) {
        let lower = task.to_lowercase();
        let mut best = ("coder", 2u32, 0usize);
        for (archetype, keywords, tier) in Self::RULES {
            let hits = keywords.iter().filter(|k| lower.contains(**k)).count();
            if hits > best.2 {
                best = (archetype, *tier, hits);
            }
        }
        // Confidence scales with the number of matched signal words (capped).
        let confidence = if best.2 == 0 {
            0.3 // default fallback to coder, low confidence
        } else {
            (0.5 + 0.15 * best.2 as f64).min(0.95)
        };
        (
            best.0.to_string(),
            Self::model_for_tier(best.1),
            best.1,
            confidence,
        )
    }
}

impl ToolHandler for HooksRouteHandler {
    fn name(&self) -> &'static str {
        "route"
    }

    fn domain(&self) -> &'static str {
        "hooks"
    }

    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("task").and_then(|v| v.as_str()).is_none() {
            return Err(crate::RuvosError::InvalidParams(
                "missing 'task' field (string)".to_string(),
            ));
        }
        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let task = params["task"].as_str().unwrap_or_default().to_string();
            let (archetype, model, tier, confidence) = Self::route(&task);
            Ok(json!({
                "task": task,
                "archetype": archetype,
                "model": model,
                "tier": tier,
                "confidence": (confidence * 1000.0).round() / 1000.0
            }))
        })
    }
}

#[cfg(test)]
mod route_tests {
    use super::*;

    #[tokio::test]
    async fn routes_testing_task_to_tester() {
        let r = HooksRouteHandler
            .execute(json!({"task": "write tests for the users endpoint"}))
            .await
            .unwrap();
        assert_eq!(r["archetype"], "tester");
        assert!(r["confidence"].as_f64().unwrap() > 0.3);
    }

    #[tokio::test]
    async fn routes_security_task_to_opus_tier() {
        let r = HooksRouteHandler
            .execute(json!({"task": "audit auth flow for injection vulnerabilities"}))
            .await
            .unwrap();
        assert_eq!(r["archetype"], "security");
        assert_eq!(r["model"], "claude-opus-4-8");
        assert_eq!(r["tier"], 3);
    }

    #[tokio::test]
    async fn routes_build_task_to_coder() {
        let r = HooksRouteHandler
            .execute(json!({"task": "implement the POST /users endpoint"}))
            .await
            .unwrap();
        assert_eq!(r["archetype"], "coder");
    }

    #[tokio::test]
    async fn unknown_task_falls_back_low_confidence() {
        let r = HooksRouteHandler
            .execute(json!({"task": "asdf qwerty zxcv"}))
            .await
            .unwrap();
        assert_eq!(r["archetype"], "coder");
        assert!(r["confidence"].as_f64().unwrap() <= 0.3);
    }

    #[test]
    fn route_requires_task() {
        assert!(HooksRouteHandler.validate(&json!({})).is_err());
        assert!(HooksRouteHandler.validate(&json!({"task": "x"})).is_ok());
    }
}
