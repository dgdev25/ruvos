//! Swarm state store for hierarchical / mesh coordination.
//!
//! The swarm layer is a thin control plane over the existing agent/orchestrate
//! primitives. It tracks membership, topology, and active objective so the
//! system can coordinate many agents as one durable unit.

use serde::{Deserialize, Serialize};

use crate::paths;
use crate::{Result, RuvosError};

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

fn swarm_path() -> std::path::PathBuf {
    paths::swarm_file()
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
}
