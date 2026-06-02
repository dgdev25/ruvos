//! Tool registry for all 20 MCP tools.

pub mod agent;
pub mod echo;
pub mod gov;
pub mod handler;
pub mod hooks;
pub mod intel;
pub mod memory;
pub mod plugin;
pub mod session;
pub mod workflow;

use serde::{Deserialize, Serialize};

pub use handler::{ToolHandler, ToolRegistry};

/// Tool metadata for registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub domain: String,
}

/// Create a new registry with all 20 tools + test tools registered.
pub fn create_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();

    // Register test tool
    registry.register(Box::new(echo::EchoHandler));

    // Register memory tools
    registry.register(Box::new(memory::MemorySearchHandler));
    registry.register(Box::new(memory::MemoryStoreHandler));
    registry.register(Box::new(memory::MemoryRetrieveHandler));
    registry.register(Box::new(memory::MemoryListHandler));

    // Register session tools
    registry.register(Box::new(session::SessionCreateStub));
    registry.register(Box::new(session::SessionResumeStub));
    registry.register(Box::new(session::SessionForkStub));

    // Register agent tools
    registry.register(Box::new(agent::AgentSpawnStub));
    registry.register(Box::new(agent::AgentStatusStub));
    registry.register(Box::new(agent::AgentMessageStub));

    // Register hooks tools
    registry.register(Box::new(hooks::HooksPreHandler::new()));
    registry.register(Box::new(hooks::HooksPostHandler::new()));
    registry.register(Box::new(hooks::HooksRouteStub));

    // Register intel tools
    registry.register(Box::new(intel::IntelPatternSearchStub));
    registry.register(Box::new(intel::IntelPatternStoreStub));

    // Register plugin tools
    registry.register(Box::new(plugin::PluginListHandler::new()));
    registry.register(Box::new(plugin::PluginInvokeHandler::new()));

    // Register gov tools
    registry.register(Box::new(gov::GovWitnessVerifyStub));
    registry.register(Box::new(gov::GovHealthStub));

    // Register workflow tools
    registry.register(Box::new(workflow::WorkflowRunStub));

    registry
}

/// Return the registry of all 20 tools (metadata only).
pub fn tool_registry() -> Vec<ToolMetadata> {
    vec![
        // Memory (4)
        ToolMetadata {
            name: "memory.search".to_string(),
            description: "Semantic search across namespaces with MMR diversity + recency weighting"
                .to_string(),
            domain: "memory".to_string(),
        },
        ToolMetadata {
            name: "memory.store".to_string(),
            description: "Insert/update an entry with optional embedding + tags".to_string(),
            domain: "memory".to_string(),
        },
        ToolMetadata {
            name: "memory.retrieve".to_string(),
            description: "Get a single entry by key".to_string(),
            domain: "memory".to_string(),
        },
        ToolMetadata {
            name: "memory.list".to_string(),
            description: "List entries in a namespace with filters".to_string(),
            domain: "memory".to_string(),
        },
        // Session (3)
        ToolMetadata {
            name: "session.create".to_string(),
            description: "Start a session, return id, persist as .rvf".to_string(),
            domain: "session".to_string(),
        },
        ToolMetadata {
            name: "session.resume".to_string(),
            description: "Restore a session by id (full context + memory)".to_string(),
            domain: "session".to_string(),
        },
        ToolMetadata {
            name: "session.fork".to_string(),
            description: "COW-branch a session for parallel exploration".to_string(),
            domain: "session".to_string(),
        },
        // Agent (3)
        ToolMetadata {
            name: "agent.spawn".to_string(),
            description: "Spawn a host agent: {host, archetype, prompt, traits, model, budget}"
                .to_string(),
            domain: "agent".to_string(),
        },
        ToolMetadata {
            name: "agent.status".to_string(),
            description: "List running agents + states".to_string(),
            domain: "agent".to_string(),
        },
        ToolMetadata {
            name: "agent.message".to_string(),
            description: "Send message to a named agent".to_string(),
            domain: "agent".to_string(),
        },
        // Hooks (3)
        ToolMetadata {
            name: "hooks.pre".to_string(),
            description: "Unified pre-hook (task|edit|command) — returns routing + context"
                .to_string(),
            domain: "hooks".to_string(),
        },
        ToolMetadata {
            name: "hooks.post".to_string(),
            description: "Unified post-hook with outcome — feeds SONA learning".to_string(),
            domain: "hooks".to_string(),
        },
        ToolMetadata {
            name: "hooks.route".to_string(),
            description: "Get model + archetype recommendation for a task".to_string(),
            domain: "hooks".to_string(),
        },
        // Intel (2)
        ToolMetadata {
            name: "intel.pattern_search".to_string(),
            description: "Find similar past trajectories (4-step retrieve phase)".to_string(),
            domain: "intel".to_string(),
        },
        ToolMetadata {
            name: "intel.pattern_store".to_string(),
            description: "Store outcome for the distill/consolidate phases".to_string(),
            domain: "intel".to_string(),
        },
        // Plugin (2)
        ToolMetadata {
            name: "plugin.list".to_string(),
            description: "Installed plugins + skills (discovered from disk)".to_string(),
            domain: "plugin".to_string(),
        },
        ToolMetadata {
            name: "plugin.invoke".to_string(),
            description: "Run a plugin command (shell exec via tokio)".to_string(),
            domain: "plugin".to_string(),
        },
        // Gov (2)
        ToolMetadata {
            name: "gov.witness_verify".to_string(),
            description: "Verify .rvf signature chain".to_string(),
            domain: "gov".to_string(),
        },
        ToolMetadata {
            name: "gov.health".to_string(),
            description: "Doctor / status across substrate, hosts, MCP, daemon".to_string(),
            domain: "gov".to_string(),
        },
        // Workflow (1)
        ToolMetadata {
            name: "workflow.run".to_string(),
            description:
                "Execute an orchestration template (feature / bugfix / refactor / security)"
                    .to_string(),
            domain: "workflow".to_string(),
        },
    ]
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_full_registry_creation() {
        let registry = create_registry();
        // All 20 tools + 1 test echo tool = 21
        assert_eq!(registry.tool_count(), 21);
    }

    #[test]
    fn test_registry_contains_all_domains() {
        let registry = create_registry();
        let tools = registry.list_tools();

        // Check each domain is represented
        assert!(tools.iter().any(|t| t.starts_with("memory.")));
        assert!(tools.iter().any(|t| t.starts_with("session.")));
        assert!(tools.iter().any(|t| t.starts_with("agent.")));
        assert!(tools.iter().any(|t| t.starts_with("hooks.")));
        assert!(tools.iter().any(|t| t.starts_with("intel.")));
        assert!(tools.iter().any(|t| t.starts_with("plugin.")));
        assert!(tools.iter().any(|t| t.starts_with("gov.")));
        assert!(tools.iter().any(|t| t.starts_with("workflow.")));
        assert!(tools.iter().any(|t| t == "echo.test"));
    }

    #[tokio::test]
    async fn test_all_stubs_execute_successfully() {
        let registry = create_registry();

        let tests = vec![
            ("echo.test", json!({"message": "test"})),
            (
                "memory.search",
                json!({"query": "test", "namespace": "test"}),
            ),
            (
                "memory.store",
                json!({"key": "test", "value": "test", "namespace": "test"}),
            ),
            (
                "memory.retrieve",
                json!({"key": "test", "namespace": "test"}),
            ),
            ("memory.list", json!({"namespace": "test"})),
            ("session.create", json!({})),
            ("agent.spawn", json!({"host": "test"})),
            ("hooks.route", json!({"task": "test"})),
            ("intel.pattern_search", json!({"query": "test"})),
            ("plugin.list", json!({})),
            ("gov.health", json!({})),
            ("workflow.run", json!({"workflow_type": "feature"})),
        ];

        for (method, params) in tests {
            let result = registry.execute(method, params).await;
            assert!(
                result.is_ok(),
                "Tool {} failed to execute: {:?}",
                method,
                result.err()
            );
        }
    }
}
