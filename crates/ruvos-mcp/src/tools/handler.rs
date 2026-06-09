// crates/ruvos-mcp/src/tools/handler.rs
//! Tool handler trait and registry for dynamic dispatch.

use crate::runtime::{
    publish_event, AutonomyMode, PolicyScope, ResourceTracker, RuntimeEvent, RuntimePolicy,
};
use crate::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::Instant;

/// Type alias for the boxed future returned by execute
pub type ExecuteFuture = Pin<Box<dyn Future<Output = Result<Value>> + Send>>;

/// Trait that all tool handlers implement.
///
/// Each tool handler is responsible for:
/// - Identifying itself by name and domain
/// - Validating input parameters
/// - Executing the tool and returning a JSON result
pub trait ToolHandler: Send + Sync {
    /// Tool name (e.g., "search", "store")
    fn name(&self) -> &'static str;

    /// Domain name (e.g., "memory", "session")
    fn domain(&self) -> &'static str;

    /// Validate parameters before execution
    fn validate(&self, params: &Value) -> Result<()>;

    /// Execute the tool with given parameters (returns a boxed async future)
    fn execute(&self, params: Value) -> ExecuteFuture;

    /// JSON Schema for the MCP inputSchema field.
    /// Default: permissive (accepts any object). Override to declare required fields
    /// so MCP clients pass typed values instead of strings.
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": true
        })
    }
}

/// Registry of all tool handlers, keyed by "domain.name"
pub struct ToolRegistry {
    handlers: HashMap<String, Box<dyn ToolHandler>>,
    policy: Option<RuntimePolicy>,
    resource_tracker: Option<Mutex<ResourceTracker>>,
}

impl ToolRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        ToolRegistry {
            handlers: HashMap::new(),
            policy: None,
            resource_tracker: None,
        }
    }

    pub fn with_policy(policy: RuntimePolicy) -> Self {
        ToolRegistry {
            handlers: HashMap::new(),
            policy: Some(policy),
            resource_tracker: None,
        }
    }

    pub fn with_resource_tracker(resource_tracker: ResourceTracker) -> Self {
        ToolRegistry {
            handlers: HashMap::new(),
            policy: None,
            resource_tracker: Some(Mutex::new(resource_tracker)),
        }
    }

    pub fn with_autonomy_mode(mode: AutonomyMode) -> Self {
        let policy = match mode {
            AutonomyMode::Autopilot => RuntimePolicy::permissive(mode),
            AutonomyMode::Manual | AutonomyMode::Assist | AutonomyMode::Delegate => {
                RuntimePolicy::restrictive(mode)
            }
        };
        Self::with_policy(policy)
    }

    /// Register a tool handler
    pub fn register(&mut self, handler: Box<dyn ToolHandler>) {
        let key = format!("{}.{}", handler.domain(), handler.name());
        self.handlers.insert(key, handler);
    }

    pub fn set_policy(&mut self, policy: RuntimePolicy) {
        self.policy = Some(policy);
    }

    pub fn set_resource_tracker(&mut self, resource_tracker: ResourceTracker) {
        self.resource_tracker = Some(Mutex::new(resource_tracker));
    }

    pub fn set_autonomy_mode(&mut self, mode: AutonomyMode) {
        let policy = match mode {
            AutonomyMode::Autopilot => RuntimePolicy::permissive(mode),
            AutonomyMode::Manual | AutonomyMode::Assist | AutonomyMode::Delegate => {
                RuntimePolicy::restrictive(mode)
            }
        };
        self.policy = Some(policy);
    }

    /// Execute a tool by method name (e.g., "memory.search")
    pub async fn execute(&self, method: &str, params: Value) -> Result<Value> {
        let started_at = Instant::now();
        let (domain, tool) = method
            .split_once('.')
            .map(|(domain, tool)| (domain.to_string(), tool.to_string()))
            .unwrap_or_else(|| ("unknown".to_string(), method.to_string()));
        let param_keys: Vec<String> = params
            .as_object()
            .map(|obj| obj.keys().cloned().collect())
            .unwrap_or_default();

        publish_event(RuntimeEvent::new(
            "tool.call.started",
            serde_json::json!({
                "method": method,
                "domain": &domain,
                "tool": &tool,
                "param_keys": param_keys,
                "param_count": params.as_object().map(|obj| obj.len()).unwrap_or(0),
            }),
        ));

        if let Some(policy) = &self.policy {
            let decision = policy.authorize(PolicyScope::Tool(method.to_string()));
            publish_event(RuntimeEvent::new(
                "policy.checked",
                serde_json::json!({
                    "method": method,
                    "allowed": decision.allowed,
                    "mode": format!("{:?}", decision.mode),
                    "scope": decision.scope,
                    "reason": decision.reason,
                }),
            ));
            if !decision.allowed {
                let elapsed_ms = started_at.elapsed().as_millis() as u64;
                publish_event(RuntimeEvent::new(
                    "tool.call.failed",
                    serde_json::json!({
                        "method": method,
                        "domain": &domain,
                        "tool": &tool,
                        "elapsed_ms": elapsed_ms,
                        "error": "permission denied",
                    }),
                ));
                return Err(crate::RuvosError::PermissionDenied(
                    "policy denied tool call".to_string(),
                ));
            }
        }

        if let Some(tracker) = &self.resource_tracker {
            if !tracker.lock().unwrap().can_start_tool() {
                publish_event(RuntimeEvent::new(
                    "resource.budget.exhausted",
                    serde_json::json!({
                        "method": method,
                        "domain": &domain,
                        "tool": &tool,
                        "reason": "tool-call budget exhausted",
                    }),
                ));
                return Err(crate::RuvosError::ValidationError(
                    "resource budget exhausted".to_string(),
                ));
            }
        }

        let result = match self.handlers.get(method) {
            Some(handler) => match handler.validate(&params) {
                Ok(()) => handler.execute(params).await,
                Err(error) => Err(error),
            },
            None => Err(crate::RuvosError::MethodNotFound),
        };

        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        if let Some(tracker) = &self.resource_tracker {
            let mut tracker = tracker.lock().unwrap();
            tracker.record_tool_call(elapsed_ms);
            publish_event(RuntimeEvent::new(
                "resource.budget.recorded",
                serde_json::json!({
                    "method": method,
                    "domain": &domain,
                    "tool": &tool,
                    "elapsed_ms": elapsed_ms,
                    "tool_calls_used": tracker.usage.tool_calls,
                    "tool_calls_limit": tracker.budget.max_tool_calls,
                    "exhausted": tracker.is_exhausted(),
                }),
            ));
        }
        match &result {
            Ok(value) => {
                let result_keys = value
                    .as_object()
                    .map(|obj| obj.keys().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                publish_event(RuntimeEvent::new(
                    "tool.call.completed",
                    serde_json::json!({
                        "method": method,
                        "domain": &domain,
                        "tool": &tool,
                        "elapsed_ms": elapsed_ms,
                        "result_keys": result_keys,
                    }),
                ));
            }
            Err(error) => {
                publish_event(RuntimeEvent::new(
                    "tool.call.failed",
                    serde_json::json!({
                        "method": method,
                        "domain": &domain,
                        "tool": &tool,
                        "elapsed_ms": elapsed_ms,
                        "error": format!("{error:?}"),
                    }),
                ));
            }
        }

        result
    }

    /// Get the count of registered tools
    pub fn tool_count(&self) -> usize {
        self.handlers.len()
    }

    /// List all registered tools
    pub fn list_tools(&self) -> Vec<String> {
        let mut tools: Vec<String> = self.handlers.keys().cloned().collect();
        tools.sort();
        tools
    }

    /// Build MCP tool definition objects (name + description + inputSchema) by
    /// combining the public tool_registry() metadata with each handler's schema().
    /// Handlers that override schema() get proper typed schemas; all others get
    /// the permissive fallback.
    pub fn mcp_tool_definitions(&self) -> Vec<serde_json::Value> {
        let metadata = crate::tools::tool_registry();
        // Build a lookup: "domain.name" -> schema from the live handler
        let mut schema_map: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();
        for (key, handler) in &self.handlers {
            schema_map.insert(key.clone(), handler.schema());
        }
        let mut defs: Vec<serde_json::Value> = metadata
            .into_iter()
            .map(|t| {
                let schema = schema_map.get(&t.name).cloned().unwrap_or_else(|| {
                    serde_json::json!({
                        "type": "object",
                        "properties": {},
                        "additionalProperties": true
                    })
                });
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "inputSchema": schema
                })
            })
            .collect();
        defs.sort_by(|a, b| {
            a["name"]
                .as_str()
                .unwrap_or("")
                .cmp(b["name"].as_str().unwrap_or(""))
        });
        defs
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn isolate() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(dir.path().to_path_buf());
        dir
    }

    #[test]
    fn test_registry_creation() {
        let registry = ToolRegistry::new();
        assert_eq!(registry.tool_count(), 0);
    }

    #[test]
    fn test_registry_register() {
        use crate::tools::echo::EchoHandler;

        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoHandler));
        assert_eq!(registry.tool_count(), 1);

        let tools = registry.list_tools();
        assert!(tools.contains(&"echo.test".to_string()));
    }

    #[tokio::test]
    async fn test_tool_execution() {
        use crate::tools::echo::EchoHandler;

        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoHandler));

        let input = json!({"message": "hello world"});
        let result = registry.execute("echo.test", input.clone()).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.get("echo").is_some());
        assert_eq!(
            response.get("echo").unwrap().as_str().unwrap(),
            "hello world"
        );
        assert!(response.get("timestamp").is_some());
        assert!(response.get("handler").is_some());
    }

    #[tokio::test]
    async fn test_method_not_found() {
        let _g = isolate();
        let registry = ToolRegistry::new();
        let result = registry.execute("nonexistent.tool", json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_emits_runtime_events() {
        let _g = isolate();
        use crate::tools::echo::EchoHandler;
        use crate::tools::gov::GovEventsHandler;

        let mut registry = ToolRegistry::new();
        registry.register(Box::new(EchoHandler));

        let response = registry
            .execute("echo.test", json!({"message": "hello"}))
            .await
            .expect("echo call must succeed");
        assert_eq!(response["echo"], "hello");

        let events = GovEventsHandler
            .execute(json!({"event_type": "tool.call.completed", "limit": 10}))
            .await
            .expect("events query must succeed");
        assert!(
            events["count"].as_u64().unwrap() >= 1,
            "tool execution must publish a completion event"
        );
        let payload = &events["events"][0]["payload"];
        assert_eq!(payload["method"], "echo.test");
        assert_eq!(payload["tool"], "test");
    }

    #[tokio::test]
    async fn restricted_policy_denies_unlisted_tool() {
        let _g = isolate();
        use crate::tools::echo::EchoHandler;

        let mut registry = ToolRegistry::with_policy(RuntimePolicy::restrictive(
            crate::runtime::AutonomyMode::Assist,
        ));
        registry.register(Box::new(EchoHandler));
        let result = registry
            .execute("echo.test", json!({"message": "hello"}))
            .await;
        assert!(matches!(
            result,
            Err(crate::RuvosError::PermissionDenied(_))
        ));
    }

    #[tokio::test]
    async fn autopilot_mode_allows_execution() {
        let _g = isolate();
        use crate::tools::echo::EchoHandler;

        let mut registry = ToolRegistry::with_autonomy_mode(AutonomyMode::Autopilot);
        registry.register(Box::new(EchoHandler));
        let result = registry
            .execute("echo.test", json!({"message": "hello"}))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn resource_tracker_blocks_after_tool_budget_exhausts() {
        let _g = isolate();
        use crate::tools::echo::EchoHandler;

        let mut registry = ToolRegistry::with_resource_tracker(ResourceTracker::restrictive(1));
        registry.register(Box::new(EchoHandler));
        assert!(registry
            .execute("echo.test", json!({"message": "hello"}))
            .await
            .is_ok());
        let second = registry
            .execute("echo.test", json!({"message": "hello"}))
            .await;
        assert!(matches!(second, Err(crate::RuvosError::ValidationError(_))));
    }
}
