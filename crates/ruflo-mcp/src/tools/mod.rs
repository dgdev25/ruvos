//! Tool registry for all 20 MCP tools.

pub mod memory;
pub mod session;
pub mod agent;
pub mod hooks;
pub mod intel;
pub mod plugin;
pub mod gov;
pub mod workflow;

use serde::{Deserialize, Serialize};

/// Tool metadata for registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub domain: String,
}

/// Return the registry of all 20 tools.
pub fn tool_registry() -> Vec<ToolMetadata> {
    vec![
        // Memory (4)
        ToolMetadata {
            name: "memory.search".to_string(),
            description: "Semantic search across namespaces with MMR diversity + recency weighting".to_string(),
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
            description: "Spawn a host agent: {host, archetype, prompt, traits, model, budget}".to_string(),
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
            description: "Unified pre-hook (task|edit|command) — returns routing + context".to_string(),
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
            description: "Execute an orchestration template (feature / bugfix / refactor / security)".to_string(),
            domain: "workflow".to_string(),
        },
    ]
}
