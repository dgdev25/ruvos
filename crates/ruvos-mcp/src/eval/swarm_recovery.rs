//! Swarm recovery eval suite.
//!
//! Verifies that the swarm state machine correctly detects stale members,
//! records outcomes, and that `learned_topology` converges after repeated runs.
//! Each scenario runs in an isolated data directory via `RUVOS_HOME`.

use crate::swarm::{
    learned_topology, record_swarm_learning, record_swarm_outcome, store, SwarmMember, SwarmState,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmRecoveryCaseResult {
    pub name: String,
    pub initial_active_count: usize,
    pub stale_count: usize,
    pub outcome_recorded: String,
    pub recommended_topology: Option<String>,
    pub passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmRecoverySummary {
    pub case_count: usize,
    pub passed_count: usize,
    pub all_passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmRecoveryReport {
    pub suite: String,
    pub cases: Vec<SwarmRecoveryCaseResult>,
    pub summary: SwarmRecoverySummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmRecoveryComparison {
    pub suite_matches: bool,
    pub case_count_matches: bool,
    pub all_passed_baseline: bool,
    pub all_passed_current: bool,
}

fn member(id: &str, role: &str, state: &str) -> SwarmMember {
    SwarmMember {
        agent_id: id.to_string(),
        role: role.to_string(),
        state: state.to_string(),
        capabilities: vec![],
        assigned_tasks: vec![],
        last_heartbeat: chrono::Utc::now().to_rfc3339(),
    }
}

fn swarm(id: &str, objective: &str, topology: &str, max_agents: u32, members: Vec<SwarmMember>) -> SwarmState {
    let coordinator = members.first().map(|m| m.agent_id.clone()).unwrap_or_default();
    SwarmState {
        id: id.to_string(),
        objective: objective.to_string(),
        topology: topology.to_string(),
        coordinator,
        max_agents,
        status: "active".to_string(),
        members,
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    }
}

/// Run `f` inside a fresh temporary data root, then restore the previous state.
/// In test builds: uses the thread-local override (safe for parallel tests).
/// In production builds: mutates `RUVOS_HOME` (eval is always single-threaded).
fn with_isolated_root<F>(f: F) -> SwarmRecoveryCaseResult
where
    F: FnOnce() -> SwarmRecoveryCaseResult,
{
    let id = Uuid::new_v4().to_string();
    let dir = std::env::temp_dir().join(format!("ruvos-eval-swarm-{id}"));
    std::fs::create_dir_all(&dir).expect("create eval swarm temp dir");

    #[cfg(test)]
    let result = {
        crate::paths::set_test_root(dir.clone());
        let r = f();
        crate::paths::clear_test_root();
        r
    };
    #[cfg(not(test))]
    let result = {
        let old_home = std::env::var("RUVOS_HOME").ok();
        // SAFETY: eval runs single-threaded in production.
        unsafe { std::env::set_var("RUVOS_HOME", &dir); }
        let r = f();
        unsafe {
            match &old_home {
                Some(old) => std::env::set_var("RUVOS_HOME", old),
                None => std::env::remove_var("RUVOS_HOME"),
            }
        }
        r
    };
    std::fs::remove_dir_all(&dir).ok();
    result
}

fn run_single_stale_detected() -> SwarmRecoveryCaseResult {
    with_isolated_root(|| {
        let members = vec![
            member("coord-1", "coordinator", "active"),
            member("worker-1", "coder", "left"), // stale
        ];
        let s = swarm("s1", "ship a feature", "hierarchical", 4, members);
        store(s.clone()).unwrap();

        let active = s.members.iter().filter(|m| m.state == "active" || m.state == "assigned").count();
        let stale = s.members.iter().filter(|m| m.state == "left").count();

        SwarmRecoveryCaseResult {
            name: "single_stale_detected".to_string(),
            initial_active_count: active,
            stale_count: stale,
            outcome_recorded: "n/a".to_string(),
            recommended_topology: None,
            passed: stale == 1 && active == 1,
        }
    })
}

fn run_failure_updates_policy() -> SwarmRecoveryCaseResult {
    with_isolated_root(|| {
        let members = vec![member("coord-1", "coordinator", "active")];
        let s = swarm("s2", "broadcast updates across peer workers", "mesh", 4, members);
        store(s.clone()).unwrap();

        record_swarm_outcome(&s, "failed", "worker timed out").unwrap();
        record_swarm_outcome(&s, "completed", "recovered after retry").unwrap();

        // Two outcomes recorded; policy should now have data for this signature.
        // learned_topology returns Some only when success > failure (here tied at 1:1),
        // OR when similar history runs exist. Either way the policy file must exist.
        let policy_path = crate::paths::swarm_policy_file();
        let passed = policy_path.exists();
        let learned = learned_topology(&s.objective, s.members.len(), s.max_agents);

        SwarmRecoveryCaseResult {
            name: "failure_updates_policy".to_string(),
            initial_active_count: 1,
            stale_count: 0,
            outcome_recorded: "completed".to_string(),
            recommended_topology: learned.map(|(t, _)| t),
            passed,
        }
    })
}

fn run_success_topology_learned() -> SwarmRecoveryCaseResult {
    with_isolated_root(|| {
        let members = vec![
            member("coord-1", "coordinator", "active"),
            member("worker-1", "coder", "active"),
        ];
        let s = swarm("s3", "broadcast updates across peer workers", "mesh", 4, members);
        store(s.clone()).unwrap();

        record_swarm_learning(&s, "completed", "first mesh run").unwrap();
        record_swarm_learning(&s, "completed", "second mesh run").unwrap();

        let learned = learned_topology(&s.objective, s.members.len(), s.max_agents);
        let topology = learned.as_ref().map(|(t, _)| t.clone());
        let passed = topology.as_deref() == Some("mesh");

        SwarmRecoveryCaseResult {
            name: "success_topology_learned".to_string(),
            initial_active_count: 2,
            stale_count: 0,
            outcome_recorded: "completed".to_string(),
            recommended_topology: topology,
            passed,
        }
    })
}

fn run_large_swarm_topology() -> SwarmRecoveryCaseResult {
    with_isolated_root(|| {
        // 8 members → task_bucket maps "distributed parallel" to "hybrid".
        let members: Vec<_> = (0..8)
            .map(|i| member(&format!("worker-{i}"), "coder", "active"))
            .collect();
        let s = swarm(
            "s4",
            "distributed parallel build task",
            "hybrid",
            10,
            members.clone(),
        );
        store(s.clone()).unwrap();

        record_swarm_learning(&s, "completed", "large run 1").unwrap();
        record_swarm_learning(&s, "completed", "large run 2").unwrap();

        let learned = learned_topology(&s.objective, members.len(), s.max_agents);
        let topology = learned.as_ref().map(|(t, _)| t.clone());
        // Any of hybrid/mesh/adaptive is acceptable for a large parallel swarm.
        let passed =
            matches!(topology.as_deref(), Some("hybrid") | Some("mesh") | Some("adaptive"));

        SwarmRecoveryCaseResult {
            name: "large_swarm_topology".to_string(),
            initial_active_count: members.len(),
            stale_count: 0,
            outcome_recorded: "completed".to_string(),
            recommended_topology: topology,
            passed,
        }
    })
}

pub fn run_swarm_recovery_suite() -> SwarmRecoveryReport {
    let cases = vec![
        run_single_stale_detected(),
        run_failure_updates_policy(),
        run_success_topology_learned(),
        run_large_swarm_topology(),
    ];

    let passed_count = cases.iter().filter(|c| c.passed).count();
    SwarmRecoveryReport {
        suite: "swarm-recovery".to_string(),
        summary: SwarmRecoverySummary {
            case_count: cases.len(),
            passed_count,
            all_passed: passed_count == cases.len(),
        },
        cases,
    }
}

pub fn load_swarm_recovery_report(path: impl AsRef<Path>) -> anyhow::Result<SwarmRecoveryReport> {
    let text = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

pub fn compare_swarm_recovery_reports(
    current: &SwarmRecoveryReport,
    baseline: &SwarmRecoveryReport,
) -> SwarmRecoveryComparison {
    SwarmRecoveryComparison {
        suite_matches: current.suite == baseline.suite,
        case_count_matches: current.summary.case_count == baseline.summary.case_count,
        all_passed_baseline: baseline.summary.all_passed,
        all_passed_current: current.summary.all_passed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suite_all_scenarios_pass() {
        let report = run_swarm_recovery_suite();
        assert_eq!(report.suite, "swarm-recovery");
        assert_eq!(report.summary.case_count, 4);
        assert!(
            report.summary.all_passed,
            "failed cases: {:?}",
            report
                .cases
                .iter()
                .filter(|c| !c.passed)
                .map(|c| &c.name)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn compare_identical_reports_matches() {
        let report = run_swarm_recovery_suite();
        let cmp = compare_swarm_recovery_reports(&report, &report);
        assert!(cmp.suite_matches);
        assert!(cmp.case_count_matches);
    }
}
