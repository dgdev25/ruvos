use crate::swarm::{self, SwarmMember, SwarmState};
use crate::{paths::ensure_root, relay};
use serde_json::{json, Value};
use uuid::Uuid;

const SCOPE_KEYWORDS: &[&str] = &[
    "refactor",
    "migrate",
    "implement",
    "integrate",
    "rewrite",
    "scaffold",
    "add feature",
    "create module",
    "build",
    "setup",
    "port",
    "upgrade",
    "update",
    "move",
    "convert",
];

const MULTI_FILE_WORDS: &[&str] = &[
    "across",
    "throughout",
    "all modules",
    "each module",
    "every module",
    "multiple files",
];

const FILE_EXTENSIONS: &[&str] = &[
    ".rs", ".ts", ".tsx", ".js", ".sql", ".toml", ".yaml", ".yml",
];

/// Returns true when the task prose looks like multi-step / multi-file work.
pub fn is_complex(prose: &str) -> bool {
    let lower = prose.to_lowercase();
    let mut signals = 0u32;

    // Signal 1: scope keyword present
    if SCOPE_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        signals += 1;
    }

    // Signal 2: multi-file indicator word
    if MULTI_FILE_WORDS.iter().any(|w| lower.contains(w)) {
        signals += 1;
    }

    // Signal 3: two or more distinct file extensions mentioned — unambiguous
    // multi-file work, so counts as two signals on its own.
    let ext_hits = FILE_EXTENSIONS
        .iter()
        .filter(|e| lower.contains(*e))
        .count();
    if ext_hits >= 2 {
        signals += 2;
    }

    // Signal 4: two or more path-like tokens (foo/bar, src/, migrations/)
    let path_hits = prose
        .split_whitespace()
        .filter(|tok| tok.contains('/') && tok.len() > 2)
        .count();
    if path_hits >= 2 {
        signals += 1;
    }

    // Signal 5: long task description
    if prose.len() >= 100 {
        signals += 1;
    }

    signals >= 2
}

/// Derive a sprint_id from the prose (uses first scope keyword found + unix seconds).
fn derive_sprint_id(prose: &str) -> String {
    let lower = prose.to_lowercase();
    let keyword = SCOPE_KEYWORDS
        .iter()
        .find(|kw| lower.contains(*kw))
        .copied()
        .unwrap_or("task");
    // Use a stable timestamp sourced from the system (not Date::now — allowed in sync Rust).
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("auto-{secs}-{}", keyword.replace(' ', "-"))
}

pub struct AutoSwarmResult {
    pub swarm_id: String,
    pub sprint_id: String,
    /// "created" | "attached" | "skipped"
    pub action: &'static str,
}

/// Probe complexity and auto-create/attach a swarm for the task.
/// Returns `action="skipped"` when the task is below threshold.
pub fn maybe_create(prose: &str, explicit_sprint_id: Option<&str>) -> AutoSwarmResult {
    if !is_complex(prose) {
        return AutoSwarmResult {
            swarm_id: String::new(),
            sprint_id: String::new(),
            action: "skipped",
        };
    }

    // Attach to existing active swarm when one is live. Cross-process lock
    // held for the whole check-then-create cycle so two sessions can't both
    // create a swarm.
    let Ok(_state_lock) = swarm::state_lock() else {
        return AutoSwarmResult {
            swarm_id: String::new(),
            sprint_id: String::new(),
            action: "skipped",
        };
    };
    if let Some(existing) = swarm::current() {
        if existing.status == "active" {
            return AutoSwarmResult {
                swarm_id: existing.id,
                sprint_id: existing.sprint_id.unwrap_or_default(),
                action: "attached",
            };
        }
    }

    let swarm_id = Uuid::new_v4().to_string();
    let sprint_id = explicit_sprint_id
        .map(String::from)
        .unwrap_or_else(|| derive_sprint_id(prose));
    let coordinator = relay::instance_id().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let state = SwarmState {
        id: swarm_id.clone(),
        objective: prose.chars().take(120).collect(),
        topology: "hierarchical".to_string(),
        coordinator: coordinator.clone(),
        max_agents: 8,
        status: "active".to_string(),
        members: vec![SwarmMember {
            agent_id: coordinator,
            role: "coordinator".to_string(),
            state: "active".to_string(),
            capabilities: vec!["orchestrate".to_string(), "route".to_string()],
            assigned_tasks: Vec::new(),
            last_heartbeat: now.clone(),
        }],
        task_graph: Default::default(),
        sprint_id: Some(sprint_id.clone()),
        baseline_tests: None,
        created_at: now.clone(),
        updated_at: now,
    };

    // Best-effort: if the data dir isn't ready we just skip persistence.
    let _ = ensure_root();
    let _ = swarm::store(state.clone());

    AutoSwarmResult {
        swarm_id,
        sprint_id,
        action: "created",
    }
}

/// Build the JSON fragment added to a hooks_pre response.
pub fn to_json(result: &AutoSwarmResult) -> Value {
    if result.action == "skipped" {
        return json!({ "swarm_action": "skipped" });
    }
    json!({
        "swarm_id":     result.swarm_id,
        "sprint_id":    result.sprint_id,
        "swarm_action": result.action,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_simple_task_is_not_complex() {
        assert!(!is_complex("fix typo in README"));
    }

    #[test]
    fn scope_keyword_plus_length_is_complex() {
        let prose = "refactor the authentication module to use the new JWT library and update all related handlers and tests to match the new API surface";
        assert!(is_complex(prose));
    }

    #[test]
    fn multi_file_extensions_is_complex() {
        assert!(is_complex(
            "update schema.sql and regenerate the .rs models and .ts client types"
        ));
    }

    #[test]
    fn multi_path_tokens_is_complex() {
        assert!(is_complex(
            "move src/handlers/ logic into crates/api/ and update migrations/ accordingly"
        ));
    }

    #[test]
    fn across_keyword_alone_insufficient() {
        // one signal only — not complex
        assert!(!is_complex("across"));
    }

    #[test]
    fn scope_keyword_plus_multi_file_word_is_complex() {
        assert!(is_complex(
            "implement the new cache layer throughout all modules"
        ));
    }

    #[test]
    fn sprint_id_contains_keyword_and_digits() {
        let id = derive_sprint_id("refactor the auth module");
        assert!(id.starts_with("auto-"));
        assert!(id.contains("refactor"));
    }
}
