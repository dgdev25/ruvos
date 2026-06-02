// crates/ruflo-mcp/src/tools/handler.rs
//! Tool handler trait and registry for dynamic dispatch.

use crate::Result;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

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
}

/// Registry of all tool handlers, keyed by "domain.name"
pub struct ToolRegistry {
    handlers: HashMap<String, Box<dyn ToolHandler>>,
}

impl ToolRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        ToolRegistry {
            handlers: HashMap::new(),
        }
    }

    /// Register a tool handler
    pub fn register(&mut self, handler: Box<dyn ToolHandler>) {
        let key = format!("{}.{}", handler.domain(), handler.name());
        self.handlers.insert(key, handler);
    }

    /// Execute a tool by method name (e.g., "memory.search")
    pub async fn execute(&self, method: &str, params: Value) -> Result<Value> {
        let handler = self
            .handlers
            .get(method)
            .ok_or(crate::RufloError::MethodNotFound)?;

        handler.validate(&params)?;
        handler.execute(params).await
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
        let registry = ToolRegistry::new();
        let result = registry.execute("nonexistent.tool", json!({})).await;
        assert!(result.is_err());
    }
}
