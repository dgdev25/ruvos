//! Tool registry for all public MCP tools.

pub mod agent;
pub mod agent_exec;
pub mod agent_store;
pub mod aisp_layer;
pub mod auto_swarm;
pub mod compress;
pub mod cve;
pub mod echo;
pub mod embedding;
pub mod gov;
pub mod gov_issues;
pub mod gov_sprint;
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

#[cfg(test)]
mod swarm_dep_tests;

use serde::{Deserialize, Serialize};

pub use handler::{ToolHandler, ToolRegistry};

/// Tool metadata for registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    pub domain: String,
}

/// Create a new registry with all public tools registered.
///
/// The `ruvos_echo_test` tool is only registered in unit tests or when
/// `RUVOS_ENABLE_TEST_TOOLS` is set — it must never appear in a production
/// `tools/list`, and it is deliberately absent from `tool_registry()` and the
/// contract manifest.
pub fn create_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();

    if cfg!(test) || std::env::var("RUVOS_ENABLE_TEST_TOOLS").is_ok() {
        registry.register(Box::new(echo::EchoHandler));
    }

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
    registry.register(Box::new(agent_exec::AgentExecHandler));

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
    registry.register(Box::new(gov_sprint::GovSprintSummaryHandler));
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

    // gov_issues (6) — ADR-028
    registry.register(Box::new(gov_issues::GovIssueCreateHandler));
    registry.register(Box::new(gov_issues::GovIssueListHandler));
    registry.register(Box::new(gov_issues::GovIssueShowHandler));
    registry.register(Box::new(gov_issues::GovIssueCloseHandler));
    registry.register(Box::new(gov_issues::GovIssueSearchHandler));
    registry.register(Box::new(gov_issues::GovIssueDepHandler));

    // Register CVE lookup tool
    registry.register(Box::new(cve::GovCveLookupHandler));

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
        // Compress (1)
        ToolMetadata {
            name: "ruvos_compress_run".to_string(),
            description:
                "Compress large text, JSON, code, or logs and return a retrieval reference"
                    .to_string(),
            domain: "compress".to_string(),
        },
        // Memory (4)
        ToolMetadata {
            name: "ruvos_memory_search".to_string(),
            description: "Hybrid search — fuses dense vectors (HNSW/RaBitQ/ACORN) with BM25 \
                          lexical ranking (RRF), then MMR diversity + recency; optional \
                          filter_tags restricts to entries with all given tags, and optional \
                          feedback:[{key,useful}] trains a bandit that reweights future results"
                .to_string(),
            domain: "memory".to_string(),
        },
        ToolMetadata {
            name: "ruvos_memory_store".to_string(),
            description: "Insert/update an entry with optional embedding + tags".to_string(),
            domain: "memory".to_string(),
        },
        ToolMetadata {
            name: "ruvos_memory_retrieve".to_string(),
            description: "Get a single entry by key".to_string(),
            domain: "memory".to_string(),
        },
        ToolMetadata {
            name: "ruvos_memory_list".to_string(),
            description: "List entries in a namespace with filters".to_string(),
            domain: "memory".to_string(),
        },
        // Session (3)
        ToolMetadata {
            name: "ruvos_session_create".to_string(),
            description: "Start a session, return id, persist as .rvf".to_string(),
            domain: "session".to_string(),
        },
        ToolMetadata {
            name: "ruvos_session_resume".to_string(),
            description: "Restore a session by id (full context + memory)".to_string(),
            domain: "session".to_string(),
        },
        ToolMetadata {
            name: "ruvos_session_fork".to_string(),
            description: "COW-branch a session for parallel exploration".to_string(),
            domain: "session".to_string(),
        },
        // Agent (3)
        ToolMetadata {
            name: "ruvos_agent_exec".to_string(),
            description: "Execute a list of typed ops (write_file/read_file/run_command/git_op) \
                          directly in ruvos — closes Gaps 1-3. sandbox:true contains file ops to \
                          a fresh temp dir (relative paths only; ../ escapes rejected). \
                          run_command is NOT OS-isolated."
                .to_string(),
            domain: "agent".to_string(),
        },
        ToolMetadata {
            name: "ruvos_agent_spawn".to_string(),
            description: "Spawn a host agent: {host, archetype, prompt, traits, model, budget}"
                .to_string(),
            domain: "agent".to_string(),
        },
        ToolMetadata {
            name: "ruvos_agent_status".to_string(),
            description: "List running agents + states".to_string(),
            domain: "agent".to_string(),
        },
        ToolMetadata {
            name: "ruvos_agent_message".to_string(),
            description: "Send message to a named agent".to_string(),
            domain: "agent".to_string(),
        },
        // Hooks (3)
        ToolMetadata {
            name: "ruvos_hooks_pre".to_string(),
            description: "Unified pre-hook (task|edit|command) — returns routing + context"
                .to_string(),
            domain: "hooks".to_string(),
        },
        ToolMetadata {
            name: "ruvos_hooks_post".to_string(),
            description: "Unified post-hook with outcome — feeds SONA learning".to_string(),
            domain: "hooks".to_string(),
        },
        ToolMetadata {
            name: "ruvos_hooks_route".to_string(),
            description: "Get model + archetype recommendation for a task".to_string(),
            domain: "hooks".to_string(),
        },
        // Intel (5)
        ToolMetadata {
            name: "ruvos_intel_pattern_search".to_string(),
            description: "Find similar past trajectories (4-step retrieve phase)".to_string(),
            domain: "intel".to_string(),
        },
        ToolMetadata {
            name: "ruvos_intel_pattern_store".to_string(),
            description: "Store outcome for the distill/consolidate phases".to_string(),
            domain: "intel".to_string(),
        },
        ToolMetadata {
            name: "ruvos_intel_intent_search".to_string(),
            description: "Search durable goals, preferences, and recurring workflows".to_string(),
            domain: "intel".to_string(),
        },
        ToolMetadata {
            name: "ruvos_intel_intent_store".to_string(),
            description: "Persist a stable goal or preference into intent memory".to_string(),
            domain: "intel".to_string(),
        },
        ToolMetadata {
            name: "ruvos_intel_repo_inspect".to_string(),
            description: "Snapshot repo health: hotspots, test gaps, and domain counts".to_string(),
            domain: "intel".to_string(),
        },
        // Plugin (2)
        ToolMetadata {
            name: "ruvos_plugin_list".to_string(),
            description: "Installed plugins + skills (discovered from disk)".to_string(),
            domain: "plugin".to_string(),
        },
        ToolMetadata {
            name: "ruvos_plugin_invoke".to_string(),
            description: "Run a plugin command via its frontmatter-declared exec entrypoint"
                .to_string(),
            domain: "plugin".to_string(),
        },
        // Gov (11) + gov_issues (6) + gov_sprint (1) = 18
        ToolMetadata {
            name: "ruvos_gov_sprint_summary".to_string(),
            description: "Aggregate sprint metrics from swarm state and event log".to_string(),
            domain: "gov".to_string(),
        },
        ToolMetadata {
            name: "ruvos_gov_issue_create".to_string(),
            description: "Create a beads_rust issue in the ruvos data root".to_string(),
            domain: "gov_issues".to_string(),
        },
        ToolMetadata {
            name: "ruvos_gov_issue_list".to_string(),
            description: "List issues with optional status/priority filters".to_string(),
            domain: "gov_issues".to_string(),
        },
        ToolMetadata {
            name: "ruvos_gov_issue_show".to_string(),
            description: "Show full issue details and comment history".to_string(),
            domain: "gov_issues".to_string(),
        },
        ToolMetadata {
            name: "ruvos_gov_issue_close".to_string(),
            description: "Close an issue with optional reason note".to_string(),
            domain: "gov_issues".to_string(),
        },
        ToolMetadata {
            name: "ruvos_gov_issue_search".to_string(),
            description: "Full-text search across issues".to_string(),
            domain: "gov_issues".to_string(),
        },
        ToolMetadata {
            name: "ruvos_gov_issue_dep".to_string(),
            description: "Add a dependency between two issues".to_string(),
            domain: "gov_issues".to_string(),
        },
        ToolMetadata {
            name: "ruvos_gov_witness_verify".to_string(),
            description: "Verify .rvf signature chain".to_string(),
            domain: "gov".to_string(),
        },
        ToolMetadata {
            name: "ruvos_gov_health".to_string(),
            description: "Doctor / status across substrate, hosts, MCP, daemon".to_string(),
            domain: "gov".to_string(),
        },
        ToolMetadata {
            name: "ruvos_gov_events".to_string(),
            description:
                "Query the signed audit/event log (since / by agent / by type) from the store"
                    .to_string(),
            domain: "gov".to_string(),
        },
        ToolMetadata {
            name: "ruvos_gov_replay".to_string(),
            description: "Replay a session or task trace from events and artifacts".to_string(),
            domain: "gov".to_string(),
        },
        ToolMetadata {
            name: "ruvos_gov_report".to_string(),
            description: "Generate a governance report with quality and benchmark signals"
                .to_string(),
            domain: "gov".to_string(),
        },
        ToolMetadata {
            name: "ruvos_gov_swarm_recommendation".to_string(),
            description: "Recommend swarm topology and assignment hints for a proposed task"
                .to_string(),
            domain: "gov".to_string(),
        },
        ToolMetadata {
            name: "ruvos_gov_swarm_plan".to_string(),
            description: "Return a concrete swarm role/phase plan for a proposed task".to_string(),
            domain: "gov".to_string(),
        },
        ToolMetadata {
            name: "ruvos_gov_swarm_status".to_string(),
            description: "Summarize the active swarm with a suggested plan overlay".to_string(),
            domain: "gov".to_string(),
        },
        ToolMetadata {
            name: "ruvos_gov_swarm_policy".to_string(),
            description: "Inspect learned swarm policy entries and topology preferences"
                .to_string(),
            domain: "gov".to_string(),
        },
        ToolMetadata {
            name: "ruvos_gov_swarm_history".to_string(),
            description: "Inspect recent swarm run history and learning outcomes".to_string(),
            domain: "gov".to_string(),
        },
        ToolMetadata {
            name: "ruvos_gov_cve_lookup".to_string(),
            description: "Scan a project directory for vulnerable dependencies via OSV/CVE"
                .to_string(),
            domain: "gov".to_string(),
        },
        // Relay (6)
        ToolMetadata {
            name: "ruvos_relay_announce".to_string(),
            description: "Register/refresh this instance's presence for cross-instance discovery"
                .to_string(),
            domain: "relay".to_string(),
        },
        ToolMetadata {
            name: "ruvos_relay_list".to_string(),
            description:
                "Discover other live instances (scope: machine|directory|repo) + drain own inbox"
                    .to_string(),
            domain: "relay".to_string(),
        },
        ToolMetadata {
            name: "ruvos_relay_send".to_string(),
            description: "Deliver a message to another instance's file mailbox by id".to_string(),
            domain: "relay".to_string(),
        },
        ToolMetadata {
            name: "ruvos_relay_contract_store".to_string(),
            description: "Persist a durable ownership / handoff contract".to_string(),
            domain: "relay".to_string(),
        },
        ToolMetadata {
            name: "ruvos_relay_contracts".to_string(),
            description: "List stored collaboration contracts".to_string(),
            domain: "relay".to_string(),
        },
        ToolMetadata {
            name: "ruvos_relay_contract_resolve".to_string(),
            description: "Resolve a contract with a decision and handoff".to_string(),
            domain: "relay".to_string(),
        },
        // Orchestrate (1)
        ToolMetadata {
            name: "ruvos_orchestrate_run".to_string(),
            description:
                "Run a multi-agent pipeline. A GOAP (A*) planner computes the archetype sequence \
                 from a template (feature/bugfix/refactor/security/sparc) or a caller-supplied \
                 goal + capabilities; static templates are the fallback"
                    .to_string(),
            domain: "orchestrate".to_string(),
        },
        // Swarm (13)
        ToolMetadata {
            name: "ruvos_swarm_create".to_string(),
            description: "Create a durable swarm with topology, roles, and objective".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "ruvos_swarm_status".to_string(),
            description: "Inspect the active swarm membership and progress".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "ruvos_swarm_assign".to_string(),
            description: "Assign a task to a swarm member and persist the handoff".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "ruvos_swarm_heartbeat".to_string(),
            description: "Refresh a swarm member heartbeat and liveness state".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "ruvos_swarm_message".to_string(),
            description: "Send a message between swarm members or broadcast to the swarm"
                .to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "ruvos_swarm_complete".to_string(),
            description: "Mark a swarm as completed and persist its final summary".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "ruvos_swarm_fail".to_string(),
            description: "Mark a swarm as failed with a recorded reason".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "ruvos_swarm_health".to_string(),
            description: "Report swarm liveness, utilization, and freshness".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "ruvos_swarm_rebalance".to_string(),
            description: "Move tasks off stale swarm members onto live members".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "ruvos_swarm_join".to_string(),
            description: "Add or reactivate a swarm member".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "ruvos_swarm_leave".to_string(),
            description: "Mark a swarm member as left".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "ruvos_swarm_report".to_string(),
            description: "Generate a swarm summary with recent activity".to_string(),
            domain: "swarm".to_string(),
        },
        ToolMetadata {
            name: "ruvos_swarm_metrics".to_string(),
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
        // All public tools plus the test echo tool (registered under cfg!(test)).
        assert_eq!(registry.tool_count(), public_tool_count() + 1);
    }

    /// The registered handlers and the hand-maintained `tool_registry()`
    /// metadata must describe the same tool surface — a handler added without
    /// metadata (or vice versa) is invisible to the contract system.
    #[test]
    fn handler_registry_matches_tool_metadata() {
        let registry = create_registry();
        let mut handlers: Vec<String> = registry
            .list_tools()
            .into_iter()
            .filter(|n| n != "ruvos_echo_test")
            .collect();
        handlers.sort();
        let metadata: Vec<String> = tool_registry().into_iter().map(|t| t.name).collect();
        assert_eq!(
            handlers, metadata,
            "create_registry() handlers and tool_registry() metadata have drifted"
        );
    }

    #[test]
    fn test_registry_contains_all_domains() {
        let registry = create_registry();
        let tools = registry.list_tools();

        // Check each domain is represented
        assert!(tools.iter().any(|t| t.starts_with("ruvos_memory_")));
        assert!(tools.iter().any(|t| t.starts_with("ruvos_session_")));
        assert!(tools.iter().any(|t| t.starts_with("ruvos_agent_")));
        assert!(tools.iter().any(|t| t.starts_with("ruvos_hooks_")));
        assert!(tools.iter().any(|t| t.starts_with("ruvos_intel_")));
        assert!(tools.iter().any(|t| t.starts_with("ruvos_plugin_")));
        assert!(tools.iter().any(|t| t.starts_with("ruvos_gov_")));
        assert!(tools.iter().any(|t| t.starts_with("ruvos_relay_")));
        assert!(tools.iter().any(|t| t.starts_with("ruvos_orchestrate_")));
        assert!(tools.iter().any(|t| t.starts_with("ruvos_swarm_")));
        assert!(tools.iter().any(|t| t == "ruvos_echo_test"));
    }

    #[tokio::test]
    async fn test_all_tools_execute_successfully() {
        // Isolate persistence to a temp dir so this doesn't touch ./.ruvos.
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(dir.path().to_path_buf());

        let registry = create_registry();

        let tests = vec![
            ("ruvos_echo_test", json!({"message": "test"})),
            (
                "ruvos_memory_store",
                json!({"key": "k", "value": "v", "namespace": "test"}),
            ),
            (
                "ruvos_memory_search",
                json!({"query": "v", "namespace": "test"}),
            ),
            (
                "ruvos_memory_retrieve",
                json!({"key": "k", "namespace": "test"}),
            ),
            ("ruvos_memory_list", json!({"namespace": "test"})),
            ("ruvos_session_create", json!({})),
            (
                "ruvos_agent_exec",
                json!({"ops": [{"op": "run_command", "cmd": "echo", "args": ["ok"]}]}),
            ),
            (
                "ruvos_agent_spawn",
                json!({"archetype": "coder", "prompt": "test", "model": "claude-haiku-4-5"}),
            ),
            ("ruvos_agent_status", json!({})),
            (
                "ruvos_hooks_route",
                json!({"task": "implement an endpoint"}),
            ),
            (
                "ruvos_intel_pattern_store",
                json!({"trajectory": ["a", "b"], "outcome": "ok"}),
            ),
            ("ruvos_intel_pattern_search", json!({"query": "a"})),
            (
                "ruvos_intel_intent_store",
                json!({"kind": "goal", "text": "ship safely", "tags": ["release"]}),
            ),
            (
                "ruvos_intel_intent_search",
                json!({"query": "ship", "kind": "goal"}),
            ),
            ("ruvos_intel_repo_inspect", json!({})),
            ("ruvos_plugin_list", json!({})),
            ("ruvos_gov_health", json!({})),
            (
                "ruvos_gov_replay",
                json!({"session_id": "00000000-0000-0000-0000-000000000000"}),
            ),
            ("ruvos_gov_report", json!({})),
            (
                "ruvos_gov_swarm_recommendation",
                json!({"objective": "broadcast updates across peer workers", "members": [{"agent_id": "worker-1", "role": "coder"}]}),
            ),
            (
                "ruvos_gov_swarm_plan",
                json!({"objective": "broadcast updates across peer workers", "members": [{"agent_id": "worker-1", "role": "coder"}]}),
            ),
            ("ruvos_gov_swarm_status", json!({})),
            ("ruvos_gov_swarm_policy", json!({})),
            ("ruvos_gov_swarm_history", json!({"limit": 5})),
            (
                "ruvos_relay_contract_store",
                json!({
                    "topic": "release",
                    "owner": "agent-a",
                    "participants": ["agent-b"],
                    "roles": [{"agent_id": "agent-a", "role": "owner", "responsibility": "ship safely"}]
                }),
            ),
            ("ruvos_relay_contracts", json!({})),
            (
                "ruvos_relay_contract_resolve",
                json!({"id": "missing", "resolution": "done"}),
            ),
            (
                "ruvos_orchestrate_run",
                json!({"template": "feature", "task": "ship it"}),
            ),
            (
                "ruvos_swarm_create",
                json!({
                    "objective": "ship it",
                    "topology": "hierarchical",
                    "members": [
                        {"agent_id": "worker-1", "role": "coder"}
                    ]
                }),
            ),
            ("ruvos_swarm_status", json!({})),
            (
                "ruvos_swarm_assign",
                json!({"agent_id": "worker-1", "task_id": "task-1"}),
            ),
            ("ruvos_swarm_heartbeat", json!({"agent_id": "worker-1"})),
            (
                "ruvos_swarm_message",
                json!({"to": "worker-1", "body": "ping"}),
            ),
            ("ruvos_swarm_complete", json!({"summary": "done"})),
            ("ruvos_swarm_fail", json!({"reason": "failed"})),
            ("ruvos_swarm_health", json!({})),
            ("ruvos_swarm_rebalance", json!({})),
            ("ruvos_swarm_join", json!({"agent_id": "worker-1"})),
            (
                "ruvos_swarm_leave",
                json!({"agent_id": "worker-1", "force": true}),
            ),
            ("ruvos_swarm_report", json!({})),
            ("ruvos_swarm_metrics", json!({})),
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
