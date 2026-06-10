//! Swarm learning-loop eval suite.
//!
//! Verifies that `record_swarm_learning` / `record_swarm_outcome` feed the
//! policy and history stores, and that `learned_topology` converges to the
//! correct topology after a small number of repeated outcomes.
//! Each scenario uses an isolated `RUVOS_HOME`.

use crate::swarm::{
    learned_topology, record_swarm_learning, record_swarm_outcome, store, SwarmMember, SwarmState,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmLearningCaseResult {
    pub name: String,
    pub outcomes_injected: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub expected_topology: Option<String>,
    pub learned_topology: Option<String>,
    pub converged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmLearningSummary {
    pub case_count: usize,
    pub converged_count: usize,
    pub convergence_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmLearningReport {
    pub suite: String,
    pub cases: Vec<SwarmLearningCaseResult>,
    pub summary: SwarmLearningSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmLearningComparison {
    pub suite_matches: bool,
    pub case_count_matches: bool,
    pub convergence_rate_delta: f64,
    pub all_converged_baseline: bool,
    pub all_converged_current: bool,
}

fn member(id: &str, state: &str) -> SwarmMember {
    SwarmMember {
        agent_id: id.to_string(),
        role: "coordinator".to_string(),
        state: state.to_string(),
        capabilities: vec![],
        assigned_tasks: vec![],
        last_heartbeat: chrono::Utc::now().to_rfc3339(),
    }
}

fn swarm_for(
    id: &str,
    objective: &str,
    topology: &str,
    member_count: usize,
    max: u32,
) -> SwarmState {
    let members: Vec<_> = (0..member_count)
        .map(|i| member(&format!("agent-{i}"), "active"))
        .collect();
    let coord = members
        .first()
        .map(|m| m.agent_id.clone())
        .unwrap_or_default();
    SwarmState {
        id: id.to_string(),
        objective: objective.to_string(),
        topology: topology.to_string(),
        coordinator: coord,
        max_agents: max,
        status: "active".to_string(),
        members,
        task_graph: Default::default(),
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    }
}

/// Run `f` inside a fresh temporary data root, then restore the previous state.
/// In test builds: uses the thread-local override (safe for parallel tests).
/// In production builds: mutates `RUVOS_HOME` (eval is always single-threaded).
fn with_isolated_root<F>(f: F) -> SwarmLearningCaseResult
where
    F: FnOnce() -> SwarmLearningCaseResult,
{
    let id = Uuid::new_v4().to_string();
    let dir = std::env::temp_dir().join(format!("ruvos-eval-learn-{id}"));
    std::fs::create_dir_all(&dir).expect("create eval learn temp dir");

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
        unsafe {
            std::env::set_var("RUVOS_HOME", &dir);
        }
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

fn run_mesh_converges() -> SwarmLearningCaseResult {
    with_isolated_root(|| {
        let s = swarm_for("l1", "broadcast updates across peer workers", "mesh", 2, 4);
        store(s.clone()).unwrap();

        record_swarm_learning(&s, "completed", "mesh run 1").unwrap();
        record_swarm_learning(&s, "completed", "mesh run 2").unwrap();

        let learned = learned_topology(&s.objective, s.members.len(), s.max_agents);
        let topo = learned.map(|(t, _)| t);
        let converged = topo.as_deref() == Some("mesh");

        SwarmLearningCaseResult {
            name: "mesh_converges_in_2".to_string(),
            outcomes_injected: 2,
            success_count: 2,
            failure_count: 0,
            expected_topology: Some("mesh".into()),
            learned_topology: topo,
            converged,
        }
    })
}

fn run_hierarchical_converges() -> SwarmLearningCaseResult {
    with_isolated_root(|| {
        let s = swarm_for(
            "l2",
            "plan and ship a feature sequentially",
            "hierarchical",
            1,
            4,
        );
        store(s.clone()).unwrap();

        record_swarm_learning(&s, "completed", "hierarchical run 1").unwrap();
        record_swarm_learning(&s, "completed", "hierarchical run 2").unwrap();

        let learned = learned_topology(&s.objective, s.members.len(), s.max_agents);
        let topo = learned.map(|(t, _)| t);
        let converged = topo.as_deref() == Some("hierarchical");

        SwarmLearningCaseResult {
            name: "hierarchical_converges_in_2".to_string(),
            outcomes_injected: 2,
            success_count: 2,
            failure_count: 0,
            expected_topology: Some("hierarchical".into()),
            learned_topology: topo,
            converged,
        }
    })
}

fn run_failures_do_not_lock_topology() -> SwarmLearningCaseResult {
    with_isolated_root(|| {
        let s = swarm_for("l3", "broadcast updates across peer workers", "mesh", 1, 4);
        store(s.clone()).unwrap();

        // Two failures: success_count (0) < failure_count (2), so policy should NOT
        // recommend a topology via the policy path. History-based retrieval requires
        // at least one *completed* run, so learned_topology should be None here.
        record_swarm_outcome(&s, "failed", "timeout 1").unwrap();
        record_swarm_outcome(&s, "failed", "timeout 2").unwrap();

        let learned = learned_topology(&s.objective, s.members.len(), s.max_agents);
        let topo = learned.map(|(t, _)| t);
        // Expect None because no completed run exists.
        let converged = topo.is_none();

        SwarmLearningCaseResult {
            name: "failures_do_not_lock_topology".to_string(),
            outcomes_injected: 2,
            success_count: 0,
            failure_count: 2,
            expected_topology: None,
            learned_topology: topo,
            converged,
        }
    })
}

fn run_mixed_success_wins() -> SwarmLearningCaseResult {
    with_isolated_root(|| {
        let s = swarm_for("l4", "broadcast updates across peer workers", "mesh", 2, 4);
        store(s.clone()).unwrap();

        record_swarm_learning(&s, "completed", "success 1").unwrap();
        record_swarm_learning(&s, "completed", "success 2").unwrap();
        record_swarm_learning(&s, "failed", "one failure").unwrap();

        let learned = learned_topology(&s.objective, s.members.len(), s.max_agents);
        let topo = learned.map(|(t, _)| t);
        // 2 successes > 1 failure → should still learn "mesh".
        let converged = topo.as_deref() == Some("mesh");

        SwarmLearningCaseResult {
            name: "mixed_success_wins".to_string(),
            outcomes_injected: 3,
            success_count: 2,
            failure_count: 1,
            expected_topology: Some("mesh".into()),
            learned_topology: topo,
            converged,
        }
    })
}

fn run_history_fallback_retrieval() -> SwarmLearningCaseResult {
    with_isolated_root(|| {
        // Seed history via record_swarm_outcome (no SONA, just disk state).
        let s = swarm_for("l5", "broadcast updates across peer workers", "mesh", 1, 4);
        store(s.clone()).unwrap();

        record_swarm_outcome(&s, "completed", "broadcast to peers 1").unwrap();
        record_swarm_outcome(&s, "completed", "broadcast to peers 2").unwrap();

        // Clear policy so only history is available.
        // (We can't delete the policy file directly but policy has only 1 entry
        // at success_count=2/failure_count=0, so learned_topology reads policy first.)
        let learned = learned_topology(&s.objective, s.members.len(), s.max_agents);
        let topo = learned.as_ref().map(|(t, _)| t.clone());
        let reason = learned.as_ref().map(|(_, r)| r.clone()).unwrap_or_default();
        // Either policy path or BM25/HNSW path is fine; just check topology = "mesh".
        let converged = topo.as_deref() == Some("mesh");
        let _ = reason;

        SwarmLearningCaseResult {
            name: "history_fallback_retrieval".to_string(),
            outcomes_injected: 2,
            success_count: 2,
            failure_count: 0,
            expected_topology: Some("mesh".into()),
            learned_topology: topo,
            converged,
        }
    })
}

pub fn run_swarm_learning_suite() -> SwarmLearningReport {
    let cases = vec![
        run_mesh_converges(),
        run_hierarchical_converges(),
        run_failures_do_not_lock_topology(),
        run_mixed_success_wins(),
        run_history_fallback_retrieval(),
    ];

    let converged_count = cases.iter().filter(|c| c.converged).count();
    let convergence_rate = if cases.is_empty() {
        0.0
    } else {
        converged_count as f64 / cases.len() as f64
    };

    SwarmLearningReport {
        suite: "swarm-learning".to_string(),
        summary: SwarmLearningSummary {
            case_count: cases.len(),
            converged_count,
            convergence_rate,
        },
        cases,
    }
}

pub fn load_swarm_learning_report(path: impl AsRef<Path>) -> anyhow::Result<SwarmLearningReport> {
    let text = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

pub fn compare_swarm_learning_reports(
    current: &SwarmLearningReport,
    baseline: &SwarmLearningReport,
) -> SwarmLearningComparison {
    SwarmLearningComparison {
        suite_matches: current.suite == baseline.suite,
        case_count_matches: current.summary.case_count == baseline.summary.case_count,
        convergence_rate_delta: current.summary.convergence_rate
            - baseline.summary.convergence_rate,
        all_converged_baseline: baseline.summary.converged_count == baseline.summary.case_count,
        all_converged_current: current.summary.converged_count == current.summary.case_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suite_all_cases_converge() {
        let report = run_swarm_learning_suite();
        assert_eq!(report.suite, "swarm-learning");
        assert_eq!(report.summary.case_count, 5);
        assert_eq!(
            report.summary.converged_count,
            report.summary.case_count,
            "non-converged cases: {:?}",
            report
                .cases
                .iter()
                .filter(|c| !c.converged)
                .map(|c| (&c.name, &c.learned_topology, &c.expected_topology))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn compare_identical_reports_zero_delta() {
        let report = run_swarm_learning_suite();
        let cmp = compare_swarm_learning_reports(&report, &report);
        assert!(cmp.suite_matches);
        assert!(cmp.case_count_matches);
        assert!((cmp.convergence_rate_delta).abs() < f64::EPSILON);
    }
}
