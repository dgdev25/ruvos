//! Swarm state store for hierarchical / mesh coordination.
//!
//! The swarm layer is a thin control plane over the existing agent/orchestrate
//! primitives. It tracks membership, topology, and active objective so the
//! system can coordinate many agents as one durable unit.

use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

use crate::paths;
use crate::tools::embedding::hnsw_rank;
use crate::tools::retrieval::bm25_rank;
use crate::{Result, RuvosError};
use ruvector_sona::{QueryTrajectory, SonaEngine, TrajectoryStep};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwarmMember {
    pub agent_id: String,
    pub role: String,
    pub state: String,
    pub capabilities: Vec<String>,
    pub assigned_tasks: Vec<String>,
    pub last_heartbeat: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwarmState {
    pub id: String,
    pub objective: String,
    pub topology: String,
    pub coordinator: String,
    pub max_agents: u32,
    pub status: String,
    pub members: Vec<SwarmMember>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SwarmPolicyEntry {
    pub signature: String,
    pub preferred_topology: String,
    pub success_count: u64,
    pub failure_count: u64,
    pub last_outcome: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SwarmPolicy {
    pub version: u32,
    pub entries: std::collections::BTreeMap<String, SwarmPolicyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SwarmRunRecord {
    pub signature: String,
    pub objective: String,
    pub topology: String,
    pub status: String,
    pub detail: String,
    pub member_count: usize,
    pub max_agents: u32,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SwarmRunHistory {
    pub version: u32,
    pub records: Vec<SwarmRunRecord>,
}

fn swarm_path() -> std::path::PathBuf {
    paths::swarm_file()
}

fn policy_path() -> std::path::PathBuf {
    paths::swarm_policy_file()
}

fn learning_path() -> std::path::PathBuf {
    paths::swarm_learning_file()
}

fn history_path() -> std::path::PathBuf {
    paths::swarm_history_file()
}

fn load() -> Option<SwarmState> {
    let bytes = std::fs::read(swarm_path()).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn save(state: &SwarmState) -> Result<()> {
    paths::ensure_root().map_err(|e| RuvosError::InternalError(format!("data dir: {e}")))?;
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|e| RuvosError::InternalError(format!("serialize swarm: {e}")))?;
    std::fs::write(swarm_path(), bytes)
        .map_err(|e| RuvosError::InternalError(format!("write swarm: {e}")))?;
    Ok(())
}

fn load_policy() -> SwarmPolicy {
    let Ok(bytes) = std::fs::read(policy_path()) else {
        return SwarmPolicy::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn save_policy(policy: &SwarmPolicy) -> Result<()> {
    paths::ensure_root().map_err(|e| RuvosError::InternalError(format!("data dir: {e}")))?;
    let bytes = serde_json::to_vec_pretty(policy)
        .map_err(|e| RuvosError::InternalError(format!("serialize swarm policy: {e}")))?;
    std::fs::write(policy_path(), bytes)
        .map_err(|e| RuvosError::InternalError(format!("write swarm policy: {e}")))?;
    Ok(())
}

fn save_learning_state(engine: &SonaEngine) -> Result<()> {
    paths::ensure_root().map_err(|e| RuvosError::InternalError(format!("data dir: {e}")))?;
    let serialized = engine.coordinator().serialize_state();
    std::fs::write(learning_path(), serialized)
        .map_err(|e| RuvosError::InternalError(format!("write swarm learning: {e}")))?;
    Ok(())
}

fn load_history() -> SwarmRunHistory {
    let Ok(bytes) = std::fs::read(history_path()) else {
        return SwarmRunHistory::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn save_history(history: &SwarmRunHistory) -> Result<()> {
    paths::ensure_root().map_err(|e| RuvosError::InternalError(format!("data dir: {e}")))?;
    let bytes = serde_json::to_vec_pretty(history)
        .map_err(|e| RuvosError::InternalError(format!("serialize swarm history: {e}")))?;
    std::fs::write(history_path(), bytes)
        .map_err(|e| RuvosError::InternalError(format!("write swarm history: {e}")))?;
    Ok(())
}

fn load_learning_state(engine: &SonaEngine) {
    if let Ok(bytes) = std::fs::read(learning_path()) {
        if let Ok(json) = String::from_utf8(bytes) {
            let _ = engine.coordinator().load_state(&json);
        }
    }
}

fn swarm_learner() -> &'static Mutex<SonaEngine> {
    static LEARNER: OnceLock<Mutex<SonaEngine>> = OnceLock::new();
    LEARNER.get_or_init(|| {
        let engine = SonaEngine::new(8);
        load_learning_state(&engine);
        Mutex::new(engine)
    })
}

fn swarm_embedding(state: &SwarmState, status: &str, detail: &str) -> Vec<f32> {
    let member_count = state.members.len() as f32;
    let active_count = state
        .members
        .iter()
        .filter(|member| member.state == "active" || member.state == "assigned")
        .count() as f32;
    let stale_count = state
        .members
        .iter()
        .filter(|member| member.state == "left")
        .count() as f32;
    let assigned_tasks = state
        .members
        .iter()
        .map(|member| member.assigned_tasks.len() as f32)
        .sum::<f32>();
    let objective_len = state.objective.len() as f32;
    let topology_score = match state.topology.as_str() {
        "mesh" => 0.8,
        "hybrid" => 0.6,
        "adaptive" => 0.9,
        _ => 0.3,
    };
    let status_score = match status {
        "completed" => 1.0,
        "failed" => -1.0,
        _ => 0.0,
    };
    let detail_len = detail.len() as f32;

    vec![
        member_count,
        active_count,
        stale_count,
        assigned_tasks,
        objective_len,
        topology_score,
        status_score,
        detail_len,
    ]
}

fn record_learning_trajectory(state: &SwarmState, status: &str, detail: &str) -> Result<()> {
    let embedding = swarm_embedding(state, status, detail);
    let quality = match status {
        "completed" => 1.0,
        "failed" => 0.0,
        _ => 0.5,
    };
    let mut trajectory = QueryTrajectory::new(
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default() as u64,
        embedding.clone(),
    );
    trajectory.add_step(TrajectoryStep::new(
        embedding.clone(),
        vec![0.25; embedding.len()],
        quality,
        0,
    ));
    trajectory.add_step(TrajectoryStep::new(
        vec![quality, state.members.len() as f32, state.max_agents as f32],
        vec![0.5, 0.5, 0.5],
        quality,
        1,
    ));
    trajectory.finalize(quality, detail.len() as u64);

    let engine = swarm_learner();
    let guard = engine
        .lock()
        .map_err(|e| RuvosError::InternalError(format!("lock swarm learner: {e}")))?;
    guard.submit_trajectory(trajectory);
    let _ = guard.force_learn();
    save_learning_state(&guard)
}

fn task_bucket(objective: &str, member_count: usize) -> String {
    let text = objective.to_lowercase();
    if text.contains("broadcast")
        || text.contains("peer")
        || text.contains("mesh")
        || text.contains("collaborat")
    {
        "mesh".to_string()
    } else if text.contains("adaptive")
        || text.contains("self-organ")
        || text.contains("self organiz")
        || text.contains("dynamic")
    {
        "adaptive".to_string()
    } else if text.contains("recovery")
        || text.contains("rebalance")
        || text.contains("stale")
        || text.contains("parallel")
        || text.contains("distributed")
        || member_count > 6
    {
        "hybrid".to_string()
    } else {
        "hierarchical".to_string()
    }
}

pub fn task_signature(objective: &str, member_count: usize) -> String {
    task_bucket(objective, member_count)
}

fn run_text(record: &SwarmRunRecord) -> String {
    [
        record.signature.as_str(),
        record.objective.as_str(),
        record.topology.as_str(),
        record.status.as_str(),
        record.detail.as_str(),
        &record.member_count.to_string(),
        &record.max_agents.to_string(),
    ]
    .join(" ")
}

fn similar_swarm_runs(
    objective: &str,
    member_count: usize,
    max_agents: u32,
    limit: usize,
) -> Vec<SwarmRunRecord> {
    let signature = task_signature(objective, member_count);
    let history = load_history();
    let candidates: Vec<SwarmRunRecord> = history
        .records
        .into_iter()
        .filter(|record| record.signature == signature)
        .collect();
    if candidates.is_empty() {
        return Vec::new();
    }

    let docs: Vec<(String, String)> = candidates
        .iter()
        .enumerate()
        .map(|(index, record)| (index.to_string(), run_text(record)))
        .collect();
    let query = format!(
        "{objective} signature {signature} member_count {member_count} max_agents {max_agents}"
    );
    let candidate_k = (limit * 4).max(limit);
    let bm25_ids = bm25_rank(&docs, &query, candidate_k);
    let dense_ids = hnsw_rank(&docs, &query, candidate_k).unwrap_or_default();

    let mut scores = std::collections::BTreeMap::<usize, usize>::new();
    for (rank, id) in bm25_ids.iter().enumerate() {
        if let Ok(index) = id.parse::<usize>() {
            scores
                .entry(index)
                .and_modify(|score| *score += candidate_k.saturating_sub(rank))
                .or_insert(candidate_k.saturating_sub(rank));
        }
    }
    for (rank, id) in dense_ids.iter().enumerate() {
        if let Ok(index) = id.parse::<usize>() {
            scores
                .entry(index)
                .and_modify(|score| *score += candidate_k.saturating_sub(rank))
                .or_insert(candidate_k.saturating_sub(rank));
        }
    }

    let mut ranked: Vec<(usize, usize)> = scores.into_iter().collect();
    ranked.sort_by(|(left_index, left_score), (right_index, right_score)| {
        right_score
            .cmp(left_score)
            .then_with(|| left_index.cmp(right_index))
    });

    ranked
        .into_iter()
        .take(limit)
        .filter_map(|(index, _)| candidates.get(index).cloned())
        .collect()
}

pub fn learned_topology(
    objective: &str,
    member_count: usize,
    max_agents: u32,
) -> Option<(String, String)> {
    let signature = task_signature(objective, member_count);
    let policy = load_policy();
    if let Some(entry) = policy.entries.get(&signature) {
        let total = entry.success_count + entry.failure_count;
        if total >= 2 && entry.success_count > entry.failure_count {
            let reason = format!(
                "learned from {} prior swarm outcomes for signature {}",
                total, entry.signature
            );
            let preferred = entry.preferred_topology.clone();
            if allowed_topology(&preferred) && max_agents > 0 {
                return Some((preferred, reason));
            }
        }
    }

    let similar = similar_swarm_runs(objective, member_count, max_agents, 6);
    if similar.is_empty() {
        return None;
    }
    let mut topology_counts = std::collections::BTreeMap::<String, (usize, usize)>::new();
    for record in similar.iter().filter(|record| record.status == "completed") {
        let entry = topology_counts
            .entry(record.topology.clone())
            .or_insert((0, 0));
        entry.0 += 1;
        if record.detail.contains("recover")
            || record.detail.contains("stale")
            || record.detail.contains("rebalance")
            || record.detail.contains("parallel")
        {
            entry.1 += 1;
        }
    }
    let Some((topology, (success_count, evidence_count))) = topology_counts.into_iter().max_by(
        |(left_topology, left_counts), (right_topology, right_counts)| {
            left_counts
                .0
                .cmp(&right_counts.0)
                .then_with(|| left_counts.1.cmp(&right_counts.1))
                .then_with(|| left_topology.cmp(right_topology))
        },
    ) else {
        return None;
    };

    if success_count == 0 || !allowed_topology(&topology) || max_agents == 0 {
        return None;
    }
    let reason = format!(
        "retrieved {} similar swarm runs via BM25/HNSW for signature {} ({} successful topology matches, {} evidence hits)",
        similar.len(),
        signature,
        success_count,
        evidence_count
    );
    Some((topology, reason))
}

pub fn recommend_plan(
    params: &serde_json::Value,
    member_count: usize,
    max_agents: u32,
) -> serde_json::Value {
    let topology = crate::tools::swarm::recommend_topology(params, member_count, max_agents);
    let objective = params
        .get("objective")
        .and_then(|v| v.as_str())
        .or_else(|| params.get("task").and_then(|v| v.as_str()))
        .or_else(|| params.get("goal").and_then(|v| v.as_str()))
        .unwrap_or_default();
    let requested_roles: Vec<String> = params
        .get("members")
        .and_then(|v| v.as_array())
        .map(|members| {
            members
                .iter()
                .filter_map(|member| member.get("role").and_then(|v| v.as_str()))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    let default_roles = match topology.topology.as_str() {
        "mesh" => vec![
            "coordinator".to_string(),
            "broadcaster".to_string(),
            "executor".to_string(),
            "checker".to_string(),
        ],
        "hybrid" => vec![
            "coordinator".to_string(),
            "planner".to_string(),
            "coder".to_string(),
            "tester".to_string(),
            "reviewer".to_string(),
            "recovery".to_string(),
        ],
        "adaptive" => vec![
            "coordinator".to_string(),
            "scout".to_string(),
            "executor".to_string(),
            "checker".to_string(),
        ],
        _ => vec![
            "coordinator".to_string(),
            "planner".to_string(),
            "coder".to_string(),
            "tester".to_string(),
            "reviewer".to_string(),
        ],
    };
    let phases = match topology.topology.as_str() {
        "mesh" => vec![
            "broadcast task intent across members",
            "fan out work to peers",
            "merge responses and reconcile conflicts",
        ],
        "hybrid" => vec![
            "plan the work under coordinator control",
            "split execution across coder and tester roles",
            "rebalance or recover stalled work if needed",
            "review and merge the outcome",
        ],
        "adaptive" => vec![
            "start with a compact coordination plan",
            "reassess topology after each checkpoint",
            "reassign roles based on live progress",
        ],
        _ => vec![
            "plan the work under coordinator control",
            "execute in a fixed handoff order",
            "verify and close out the result",
        ],
    };
    let assignment_hint = match topology.topology.as_str() {
        "mesh" => {
            "Prioritize direct peer coordination, broadcast updates, and parallel confirmations."
        }
        "hybrid" => {
            "Keep a coordinator-led plan, but expect parallel coder/tester loops and recovery."
        }
        "adaptive" => "Keep roles fluid and revisit the plan at checkpoints.",
        _ => "Keep a coordinator-led handoff chain with minimal parallelism.",
    };

    serde_json::json!({
        "objective": objective,
        "member_count": member_count,
        "max_agents": max_agents,
        "recommended_topology": topology.topology,
        "topology_source": topology.source,
        "topology_reason": topology.reason,
        "assignment_hint": assignment_hint,
        "default_roles": default_roles,
        "provided_roles": requested_roles,
        "phases": phases,
    })
}

pub fn record_swarm_outcome(
    state: &SwarmState,
    status: &str,
    detail: impl AsRef<str>,
) -> Result<()> {
    let signature = task_signature(&state.objective, state.members.len());
    let mut policy = load_policy();
    let entry = policy
        .entries
        .entry(signature.clone())
        .or_insert_with(|| SwarmPolicyEntry {
            signature: signature.clone(),
            preferred_topology: state.topology.clone(),
            success_count: 0,
            failure_count: 0,
            last_outcome: String::new(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        });
    match status {
        "completed" => {
            entry.success_count = entry.success_count.saturating_add(1);
        }
        "failed" => {
            entry.failure_count = entry.failure_count.saturating_add(1);
        }
        _ => {}
    }
    let success_total = entry.success_count;
    let failure_total = entry.failure_count;
    if success_total > failure_total {
        entry.preferred_topology = state.topology.clone();
    }
    entry.last_outcome = detail.as_ref().to_string();
    entry.updated_at = chrono::Utc::now().to_rfc3339();
    policy.version = policy.version.saturating_add(1);
    save_policy(&policy)?;

    let mut history = load_history();
    history.records.push(SwarmRunRecord {
        signature,
        objective: state.objective.clone(),
        topology: state.topology.clone(),
        status: status.to_string(),
        detail: detail.as_ref().to_string(),
        member_count: state.members.len(),
        max_agents: state.max_agents,
        updated_at: chrono::Utc::now().to_rfc3339(),
    });
    history.version = history.version.saturating_add(1);
    save_history(&history)
}

pub fn record_swarm_learning(
    state: &SwarmState,
    status: &str,
    detail: impl AsRef<str>,
) -> Result<()> {
    record_swarm_outcome(state, status, detail.as_ref())?;
    record_learning_trajectory(state, status, detail.as_ref())
}

fn allowed_topology(topology: &str) -> bool {
    matches!(topology, "hierarchical" | "mesh" | "hybrid" | "adaptive")
}

pub fn store(state: SwarmState) -> Result<SwarmState> {
    save(&state)?;
    Ok(state)
}

pub fn current() -> Option<SwarmState> {
    load()
}

pub fn validate_topology(topology: &str) -> Result<()> {
    if allowed_topology(topology) {
        Ok(())
    } else {
        Err(RuvosError::InvalidParams(format!(
            "invalid topology '{topology}' (expected hierarchical|mesh|hybrid|adaptive)"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolate() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(dir.path().to_path_buf());
        dir
    }

    #[test]
    fn swarm_state_roundtrips() {
        let _g = isolate();
        let state = SwarmState {
            id: "swarm-1".into(),
            objective: "ship feature".into(),
            topology: "hierarchical".into(),
            coordinator: "coord-1".into(),
            max_agents: 4,
            status: "active".into(),
            members: vec![SwarmMember {
                agent_id: "coord-1".into(),
                role: "coordinator".into(),
                state: "active".into(),
                capabilities: vec!["orchestrate".into()],
                assigned_tasks: vec![],
                last_heartbeat: chrono::Utc::now().to_rfc3339(),
            }],
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        let stored = store(state).unwrap();
        let loaded = current().unwrap();
        assert_eq!(loaded.id, stored.id);
        assert!(validate_topology("mesh").is_ok());
        assert!(validate_topology("bad").is_err());
    }

    #[test]
    fn swarm_learning_records_outcomes_and_influences_topology() {
        let _g = isolate();
        let state = SwarmState {
            id: "swarm-learn".into(),
            objective: "broadcast updates across peer workers".into(),
            topology: "mesh".into(),
            coordinator: "coord-1".into(),
            max_agents: 4,
            status: "completed".into(),
            members: vec![SwarmMember {
                agent_id: "coord-1".into(),
                role: "coordinator".into(),
                state: "active".into(),
                capabilities: vec!["orchestrate".into()],
                assigned_tasks: vec!["task-1".into()],
                last_heartbeat: chrono::Utc::now().to_rfc3339(),
            }],
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };

        record_swarm_learning(&state, "completed", "first run").unwrap();
        record_swarm_learning(&state, "completed", "second run").unwrap();

        let learned = learned_topology(&state.objective, state.members.len(), state.max_agents)
            .expect("expected learned topology after repeated success");
        assert_eq!(learned.0, "mesh");
        assert!(crate::paths::swarm_policy_file().exists());
        assert!(crate::paths::swarm_learning_file().exists());
    }

    #[test]
    fn similar_runs_influence_topology_when_policy_is_absent() {
        let _g = isolate();
        let state = SwarmState {
            id: "swarm-similar".into(),
            objective: "broadcast updates across peer workers".into(),
            topology: "mesh".into(),
            coordinator: "coord-1".into(),
            max_agents: 4,
            status: "completed".into(),
            members: vec![SwarmMember {
                agent_id: "coord-1".into(),
                role: "coordinator".into(),
                state: "active".into(),
                capabilities: vec!["orchestrate".into()],
                assigned_tasks: vec!["task-1".into()],
                last_heartbeat: chrono::Utc::now().to_rfc3339(),
            }],
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };

        let mut history = SwarmRunHistory::default();
        history.records.push(SwarmRunRecord {
            signature: task_signature(&state.objective, state.members.len()),
            objective: "broadcast updates across peer workers".into(),
            topology: "mesh".into(),
            status: "completed".into(),
            detail: "broadcast to peers".into(),
            member_count: 1,
            max_agents: 4,
            updated_at: chrono::Utc::now().to_rfc3339(),
        });
        history.records.push(SwarmRunRecord {
            signature: task_signature(&state.objective, state.members.len()),
            objective: "broadcast updates across peer workers".into(),
            topology: "mesh".into(),
            status: "completed".into(),
            detail: "peer mesh coordination".into(),
            member_count: 1,
            max_agents: 4,
            updated_at: chrono::Utc::now().to_rfc3339(),
        });
        save_history(&history).unwrap();

        let learned = learned_topology(&state.objective, state.members.len(), state.max_agents)
            .expect("expected learned topology from similar runs");
        assert_eq!(learned.0, "mesh");
        assert!(learned.1.contains("BM25/HNSW"));
    }
}
