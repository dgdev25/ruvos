//! Tool registry for all public MCP tools.

pub mod agent;
pub mod agent_store;
pub mod compress;
pub mod echo;
pub mod embedding;
pub mod gov;
pub mod handler;
pub mod hooks;
pub mod hooks_route;
pub mod intel;
pub mod memory;
pub mod orchestrate;
pub mod orchestrate_plan;
pub mod plugin;
pub mod relay;
pub mod retrieval;
pub mod session;
pub mod swarm;

use serde::{Deserialize, Serialize};

pub use handler::{ToolHandler, ToolRegistry};

/// Tool metadata for registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub domain: String,
}

/// Create a new registry with all core tools + test tools registered.
pub fn create_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();

    // Register test tool
    registry.register(Box::new(echo::EchoHandler));

    // Register compression tools
    registry.register(Box::new(compress::CompressRunHandler));

    // Register memory tools
    registry.register(Box::new(memory::MemorySearchHandler));
    registry.register(Box::new(memory::MemoryStoreHandler));
    registry.register(Box::new(memory::MemoryRetrieveHandler));
    registry.register(Box::new(memory::MemoryListHandler));

    // Register session tools
    registry.register(Box::new(session::SessionCreateHandler));
    registry.register(Box::new(session::SessionResumeHandler));
    registry.register(Box::new(session::SessionForkHandler));

    // Register agent tools
    registry.register(Box::new(agent::AgentSpawnHandler));
    registry.register(Box::new(agent::AgentStatusHandler));
    registry.register(Box::new(agent::AgentMessageHandler));

    // Register hooks tools
    registry.register(Box::new(hooks::HooksPreHandler::new()));
    registry.register(Box::new(hooks::HooksPostHandler::new()));
    registry.register(Box::new(hooks::HooksRouteHandler));

    // Register intel tools
    registry.register(Box::new(intel::IntelPatternSearchHandler));
    registry.register(Box::new(intel::IntelPatternStoreHandler));
    registry.register(Box::new(intel::IntelIntentSearchHandler));
    registry.register(Box::new(intel::IntelIntentStoreHandler));
    registry.register(Box::new(intel::IntelRepoInspectHandler));

    // Register plugin tools
    registry.register(Box::new(plugin::PluginListHandler::new()));
    registry.register(Box::new(plugin::PluginInvokeHandler::new()));

    // Register gov tools
    registry.register(Box::new(gov::GovWitnessVerifyHandler));
    registry.register(Box::new(gov::GovHealthHandler));
    registry.register(Box::new(gov::GovEventsHandler));
    registry.register(Box::new(gov::GovReplayHandler));
    registry.register(Box::new(gov::GovReportHandler));
    registry.register(Box::new(gov::GovSwarmRecommendationHandler));
    registry.register(Box::new(gov::GovSwarmPlanHandler));
    registry.register(Box::new(gov::GovSwarmStatusHandler));
    registry.register(Box::new(gov::GovSwarmPolicyHandler));
    registry.register(Box::new(gov::GovSwarmHistoryHandler));

    // Register relay tools
    registry.register(Box::new(relay::RelayAnnounceHandler));
    registry.register(Box::new(relay::RelayListHandler));
    registry.register(Box::new(relay::RelaySendHandler));
    registry.register(Box::new(relay::RelayContractStoreHandler));
    registry.register(Box::new(relay::RelayContractsHandler));
    registry.register(Box::new(relay::RelayContractResolveHandler));

    // Register orchestrate tools
    registry.register(Box::new(orchestrate::OrchestrateRunHandler));

    // Register swarm tools
    registry.register(Box::new(swarm::SwarmCreateHandler));
    registry.register(Box::new(swarm::SwarmStatusHandler));
    registry.register(Box::new(swarm::SwarmAssignHandler));
    registry.register(Box::new(swarm::SwarmHeartbeatHandler));
    registry.register(Box::new(swarm::SwarmMessageHandler));
    registry.register(Box::new(swarm::SwarmCompleteHandler));
    registry.register(Box::new(swarm::SwarmFailHandler));
    registry.register(Box::new(swarm::SwarmHealthHandler));
    registry.register(Box::new(swarm::SwarmRebalanceHandler));
    registry.register(Box::new(swarm::SwarmJoinHandler));
    registry.register(Box::new(swarm::SwarmLeaveHandler));
    registry.register(Box::new(swarm::SwarmReportHandler));
    registry.register(Box::new(swarm::SwarmMetricsHandler));

    registry
}

/// Return the registry of all tools (metadata only).
pub fn tool_registry() -> Vec<ToolMetadata> {
    let mut tools = vec![
        // Memory (4)
        ToolMetadata {
            name: "compress.run".to_string(),
            description:
                "Compress large text, JSON, code, or logs and return a retrieval reference"
                    .to_string(),
            domain: "compress".to_string(),
        },
        ToolMetadata {
            name: "memory.search".to_string(),
            description: "Hybrid search — fuses dense vectors (HNSW/RaBitQ/ACORN) with BM25 \
                          lexical ranking (RRF), then MMR diversity + recency; optional \
                          filter_tags restricts to entries with all given tags, and optional \
                          feedback:[{key,useful}] trains a bandit that reweights future results"
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
        // Intel (5)
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
        ToolMetadata {
            name: "intel.intent_search".to_string(),
            description: "Search durable goals, preferences, and recurring workflows".to_string(),
            domain: "intel".to_string(),
        },
        ToolMetadata {
            name: "intel.intent_store".to_string(),
            description: "Persist a stable goal or preference into intent memory".to_string(),
            domain: "intel".to_string(),
        },
        ToolMetadata {
            name: "intel.repo_inspect".to_string(),
            description: "Snapshot repo health: hotspots, test gaps, and domain counts".to_string(),
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
        // Gov (10)
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
        ToolMetadata {
            name: "gov.events".to_string(),
            description:
                "Query the signed audit/event log (since / by agent / by type) from the store"
                    .to_string(),
            domain: "gov".to_string(),
        },
        ToolMetadata {
            name: "gov.replay".to_string(),
            description: "Replay a session or task trace from events and artifacts".to_string(),
            domain: "gov".to_string(),
        },
        ToolMetadata {
            name: "gov.report".to_string(),
            description: "Generate a governance report with quality and benchmark signals"
                .to_string(),
            domain: "gov".to_string(),
        },
        ToolMetadata {
            name: "gov.swarm_recommendation".to_string(),
            description: "Recommend swarm topology and assignment hints for a proposed task"
                .to_string(),
            domain: "gov".to_string(),
        },
        ToolMetadata {
            name: "gov.swarm_plan".to_string(),
            description: "Return a concrete swarm role/phase plan for a proposed task".to_string(),
            domain: "gov".to_string(),
        },
        ToolMetadata {
            name: "gov.swarm_status".to_string(),
            description: "Summarize the active swarm with a suggested plan overlay".to_string(),
            domain: "gov".to_string(),
        },
        ToolMetadata {
            name: "gov.swarm_policy".to_string(),
            description: "Inspect learned swarm policy entries and topology preferences"
                .to_string(),
            domain: "gov".to_string(),
        },
        ToolMetadata {
            name: "gov.swarm_history".to_string(),
            description: "Inspect recent swarm run history and learning outcomes".to_string(),
            domain: "gov".to_string(),
        },
        // Relay (6)
        ToolMetadata {
            name: "relay.announce".to_string(),
            description: "Register/refresh this instance's presence for cross-instance discovery"
                .to_string(),
            domain: "relay".to_string(),
        },
        ToolMetadata {
            name: "relay.list".to_string(),
            description:
                "Discover other live instances (scope: machine|directory|repo) + drain own inbox"
                    .to_string(),
            domain: "relay".to_string(),
        },
        ToolMetadata {
            name: "relay.send".to_string(),
            description: "Deliver a message to another instance's file mailbox by id".to_string(),
            domain: "relay".to_string(),
        },
        ToolMetadata {
            name: "relay.contract_store".to_string(),
            description: "Persist a durable ownership / handoff contract".to_string(),
            domain: "relay".to_string(),
        },
        ToolMetadata {
            name: "relay.contracts".to_string(),
            description: "List stored collaboration contracts".to_string(),
            domain: "relay".to_string(),
        },
        ToolMetadata {
            name: "relay.contract_resolve".to_string(),
            description: "Resolve a contract with a decision and handoff".to_string(),
            domain: "relay".to_string(),
        },
        // Orchestrate (1)
        ToolMetadata {
            name: "orchestrate.run".to_string(),
            description:
                "Run a multi-agent pipeline. A GOAP (A*) planner computes the archetype sequence \
                 from a template (feature/bugfix/refactor/security/sparc) or a caller-supplied \
                 goal + capabilities; static templates are the fallback"
                    .to_string(),
            domain: "orchestrate".to_string(),
        },
        // Swarm (2)
        ToolMetadata {
            name: "swarm.create".to_string(),
            description: "Create a durable swarm with topology, roles, and objective".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "swarm.status".to_string(),
            description: "Inspect the active swarm membership and progress".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "swarm.assign".to_string(),
            description: "Assign a task to a swarm member and persist the handoff".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "swarm.heartbeat".to_string(),
            description: "Refresh a swarm member heartbeat and liveness state".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "swarm.message".to_string(),
            description: "Send a message between swarm members or broadcast to the swarm"
                .to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "swarm.complete".to_string(),
            description: "Mark a swarm as completed and persist its final summary".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "swarm.fail".to_string(),
            description: "Mark a swarm as failed with a recorded reason".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "swarm.health".to_string(),
            description: "Report swarm liveness, utilization, and freshness".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "swarm.rebalance".to_string(),
            description: "Move tasks off stale swarm members onto live members".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "swarm.join".to_string(),
            description: "Add or reactivate a swarm member".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "swarm.leave".to_string(),
            description: "Mark a swarm member as left".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "swarm.report".to_string(),
            description: "Generate a swarm summary with recent activity".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "swarm.metrics".to_string(),
            description: "Return numeric swarm health and throughput metrics".to_string(),
            domain: "swarm".to_string(),
        },
    ];
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    tools
}

/// Return the number of public MCP tools currently advertised.
pub fn public_tool_count() -> usize {
    tool_registry().len()
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_full_registry_creation() {
        let registry = create_registry();
        // All public tools plus the registry's test echo tool.
        assert_eq!(registry.tool_count(), public_tool_count() + 1);
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
        assert!(tools.iter().any(|t| t.starts_with("relay.")));
        assert!(tools.iter().any(|t| t.starts_with("orchestrate.")));
        assert!(tools.iter().any(|t| t.starts_with("swarm.")));
        assert!(tools.iter().any(|t| t == "echo.test"));
    }

    #[tokio::test]
    async fn test_all_tools_execute_successfully() {
        // Isolate persistence to a temp dir so this doesn't touch ./.ruvos.
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(dir.path().to_path_buf());

        let registry = create_registry();

        let tests = vec![
            ("echo.test", json!({"message": "test"})),
            (
                "memory.store",
                json!({"key": "k", "value": "v", "namespace": "test"}),
            ),
            ("memory.search", json!({"query": "v", "namespace": "test"})),
            ("memory.retrieve", json!({"key": "k", "namespace": "test"})),
            ("memory.list", json!({"namespace": "test"})),
            ("session.create", json!({})),
            (
                "agent.spawn",
                json!({"archetype": "coder", "prompt": "test", "model": "claude-haiku-4-5"}),
            ),
            ("agent.status", json!({})),
            ("hooks.route", json!({"task": "implement an endpoint"})),
            (
                "intel.pattern_store",
                json!({"trajectory": ["a", "b"], "outcome": "ok"}),
            ),
            ("intel.pattern_search", json!({"query": "a"})),
            (
                "intel.intent_store",
                json!({"kind": "goal", "text": "ship safely", "tags": ["release"]}),
            ),
            (
                "intel.intent_search",
                json!({"query": "ship", "kind": "goal"}),
            ),
            ("intel.repo_inspect", json!({})),
            ("plugin.list", json!({})),
            ("gov.health", json!({})),
            (
                "gov.replay",
                json!({"session_id": "00000000-0000-0000-0000-000000000000"}),
            ),
            ("gov.report", json!({})),
            (
                "gov.swarm_recommendation",
                json!({"objective": "broadcast updates across peer workers", "members": [{"agent_id": "worker-1", "role": "coder"}]}),
            ),
            (
                "gov.swarm_plan",
                json!({"objective": "broadcast updates across peer workers", "members": [{"agent_id": "worker-1", "role": "coder"}]}),
            ),
            ("gov.swarm_status", json!({})),
            ("gov.swarm_policy", json!({})),
            ("gov.swarm_history", json!({"limit": 5})),
            (
                "relay.contract_store",
                json!({
                    "topic": "release",
                    "owner": "agent-a",
                    "participants": ["agent-b"],
                    "roles": [{"agent_id": "agent-a", "role": "owner", "responsibility": "ship safely"}]
                }),
            ),
            ("relay.contracts", json!({})),
            (
                "relay.contract_resolve",
                json!({"id": "missing", "resolution": "done"}),
            ),
            (
                "orchestrate.run",
                json!({"template": "feature", "task": "ship it"}),
            ),
            (
                "swarm.create",
                json!({
                    "objective": "ship it",
                    "topology": "hierarchical",
                    "members": [
                        {"agent_id": "worker-1", "role": "coder"}
                    ]
                }),
            ),
            ("swarm.status", json!({})),
            (
                "swarm.assign",
                json!({"agent_id": "worker-1", "task_id": "task-1"}),
            ),
            ("swarm.heartbeat", json!({"agent_id": "worker-1"})),
            ("swarm.message", json!({"to": "worker-1", "body": "ping"})),
            ("swarm.complete", json!({"summary": "done"})),
            ("swarm.fail", json!({"reason": "failed"})),
            ("swarm.health", json!({})),
            ("swarm.rebalance", json!({})),
            ("swarm.join", json!({"agent_id": "worker-1"})),
            (
                "swarm.leave",
                json!({"agent_id": "worker-1", "force": true}),
            ),
            ("swarm.report", json!({})),
            ("swarm.metrics", json!({})),
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
