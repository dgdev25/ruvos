//! Tests for ADR-023: task dependency graph in swarm coordination.
use super::swarm::{
    SwarmAssignHandler, SwarmCreateHandler, SwarmHeartbeatHandler, SwarmStatusHandler,
};
use crate::tools::handler::ToolHandler;
use serde_json::json;

fn isolate() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    crate::paths::set_test_root(dir.path().to_path_buf());
    dir
}

async fn create_swarm_with_workers() -> (String, String, String) {
    let created = SwarmCreateHandler
        .execute(json!({
            "objective": "sprint pipeline",
            "topology": "hierarchical",
            "members": [
                {"agent_id": "tester-1",  "role": "tester",  "capabilities": []},
                {"agent_id": "coder-1",   "role": "coder",   "capabilities": []},
                {"agent_id": "reviewer-1","role": "reviewer","capabilities": []}
            ]
        }))
        .await
        .unwrap();
    let swarm_id = created["swarm"]["id"].as_str().unwrap().to_string();
    (swarm_id, "tester-1".into(), "coder-1".into())
}

// ── no-dependency path (backward compat) ─────────────────────────────────────

#[tokio::test]
async fn assign_without_deps_works_normally() {
    let _g = isolate();
    let (_, tester, _) = create_swarm_with_workers().await;
    let r = SwarmAssignHandler
        .execute(json!({"agent_id": tester, "task_id": "write-tests"}))
        .await
        .unwrap();
    assert_eq!(r["status"], "assigned");
}

// ── satisfied dependencies ────────────────────────────────────────────────────

#[tokio::test]
async fn assign_with_all_deps_completed_assigns_immediately() {
    let _g = isolate();
    let (_, tester, coder) = create_swarm_with_workers().await;

    // Register "write-tests" and mark it completed via heartbeat.
    SwarmAssignHandler
        .execute(json!({"agent_id": tester, "task_id": "write-tests"}))
        .await
        .unwrap();
    SwarmHeartbeatHandler
        .execute(json!({"agent_id": tester, "status": "completed"}))
        .await
        .unwrap();

    // "implement" depends on "write-tests" (now Completed) → should assign directly.
    let r = SwarmAssignHandler
        .execute(json!({
            "agent_id": coder,
            "task_id": "implement",
            "depends_on": ["write-tests"]
        }))
        .await
        .unwrap();
    assert_eq!(
        r["status"], "assigned",
        "all deps completed → assign immediately"
    );
}

// ── unsatisfied dependencies ──────────────────────────────────────────────────

#[tokio::test]
async fn assign_with_pending_dep_returns_queued() {
    let _g = isolate();
    let (_, tester, coder) = create_swarm_with_workers().await;

    // Register "write-tests" but do NOT complete it.
    SwarmAssignHandler
        .execute(json!({"agent_id": tester, "task_id": "write-tests"}))
        .await
        .unwrap();

    let r = SwarmAssignHandler
        .execute(json!({
            "agent_id": coder,
            "task_id": "implement",
            "depends_on": ["write-tests"]
        }))
        .await
        .unwrap();
    assert_eq!(r["status"], "queued");
    let blocked = r["blocked_by"].as_array().unwrap();
    assert!(blocked.iter().any(|v| v == "write-tests"));
}

#[tokio::test]
async fn queued_task_not_in_member_assigned_tasks() {
    let _g = isolate();
    let (_, tester, coder) = create_swarm_with_workers().await;

    SwarmAssignHandler
        .execute(json!({"agent_id": tester, "task_id": "write-tests"}))
        .await
        .unwrap();
    SwarmAssignHandler
        .execute(json!({
            "agent_id": coder,
            "task_id": "implement",
            "depends_on": ["write-tests"]
        }))
        .await
        .unwrap();

    let status = SwarmStatusHandler.execute(json!({})).await.unwrap();
    let coder_member = status["swarm"]["members"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["agent_id"] == coder)
        .unwrap()
        .clone();
    let assigned = coder_member["assigned_tasks"].as_array().unwrap();
    assert!(
        !assigned.iter().any(|t| t == "implement"),
        "blocked task must not appear in assigned_tasks"
    );
}

// ── failed dependency cascade ─────────────────────────────────────────────────

#[tokio::test]
async fn assign_with_failed_dep_returns_dependency_failed() {
    let _g = isolate();
    let (_, tester, coder) = create_swarm_with_workers().await;

    SwarmAssignHandler
        .execute(json!({"agent_id": tester, "task_id": "write-tests"}))
        .await
        .unwrap();
    SwarmHeartbeatHandler
        .execute(json!({"agent_id": tester, "status": "failed"}))
        .await
        .unwrap();

    let r = SwarmAssignHandler
        .execute(json!({
            "agent_id": coder,
            "task_id": "implement",
            "depends_on": ["write-tests"]
        }))
        .await
        .unwrap();
    assert_eq!(r["status"], "dependency_failed");
    let blocked = r["blocked_by"].as_array().unwrap();
    assert!(blocked.iter().any(|v| v == "write-tests"));
}

#[tokio::test]
async fn heartbeat_failed_cascades_to_waiting_dependents() {
    let _g = isolate();
    let (_, tester, coder) = create_swarm_with_workers().await;

    // write-tests assigned; implement queued behind it.
    SwarmAssignHandler
        .execute(json!({"agent_id": tester, "task_id": "write-tests"}))
        .await
        .unwrap();
    SwarmAssignHandler
        .execute(json!({
            "agent_id": coder,
            "task_id": "implement",
            "depends_on": ["write-tests"]
        }))
        .await
        .unwrap();

    // Tester fails → write-tests goes Failed → implement should auto-fail.
    SwarmHeartbeatHandler
        .execute(json!({"agent_id": tester, "status": "failed"}))
        .await
        .unwrap();

    let status = SwarmStatusHandler.execute(json!({})).await.unwrap();
    let tasks = status["tasks"].as_array().unwrap();
    let implement = tasks
        .iter()
        .find(|t| t["task_id"] == "implement")
        .expect("implement must appear in tasks");
    assert_eq!(
        implement["state"], "failed",
        "cascade must propagate failure"
    );
}

// ── cycle detection ───────────────────────────────────────────────────────────

#[tokio::test]
async fn assign_self_dependency_returns_cycle_error() {
    let _g = isolate();
    let (_, tester, _) = create_swarm_with_workers().await;
    SwarmAssignHandler
        .execute(json!({"agent_id": tester, "task_id": "write-tests"}))
        .await
        .unwrap();
    let r = SwarmAssignHandler
        .execute(json!({
            "agent_id": tester,
            "task_id": "write-tests",
            "depends_on": ["write-tests"]
        }))
        .await;
    assert!(r.is_err(), "self-dependency must be rejected as a cycle");
}

#[tokio::test]
async fn assign_mutual_dependency_returns_cycle_error() {
    let _g = isolate();
    let (_, tester, coder) = create_swarm_with_workers().await;

    SwarmAssignHandler
        .execute(json!({"agent_id": tester, "task_id": "t1"}))
        .await
        .unwrap();
    SwarmAssignHandler
        .execute(json!({
            "agent_id": coder,
            "task_id": "t2",
            "depends_on": ["t1"]
        }))
        .await
        .unwrap();

    // Adding t1 → t2 creates t1 ↔ t2 cycle.
    let r = SwarmAssignHandler
        .execute(json!({
            "agent_id": tester,
            "task_id": "t1",
            "depends_on": ["t2"]
        }))
        .await;
    assert!(r.is_err(), "mutual dependency must be detected as a cycle");
}

// ── unknown dependency ────────────────────────────────────────────────────────

#[tokio::test]
async fn assign_unknown_dep_returns_error() {
    let _g = isolate();
    let (_, _, coder) = create_swarm_with_workers().await;
    let r = SwarmAssignHandler
        .execute(json!({
            "agent_id": coder,
            "task_id": "implement",
            "depends_on": ["nonexistent-task"]
        }))
        .await;
    assert!(r.is_err(), "unknown dependency must be rejected");
    assert!(
        r.unwrap_err().to_string().contains("not registered"),
        "error message must mention registration"
    );
}

// ── status blocked_by view ────────────────────────────────────────────────────

#[tokio::test]
async fn status_tasks_field_shows_blocked_by() {
    let _g = isolate();
    let (_, tester, coder) = create_swarm_with_workers().await;

    SwarmAssignHandler
        .execute(json!({"agent_id": tester, "task_id": "write-tests"}))
        .await
        .unwrap();
    SwarmAssignHandler
        .execute(json!({
            "agent_id": coder,
            "task_id": "implement",
            "depends_on": ["write-tests"]
        }))
        .await
        .unwrap();

    let status = SwarmStatusHandler.execute(json!({})).await.unwrap();
    let tasks = status["tasks"].as_array().expect("tasks must be an array");

    let implement = tasks
        .iter()
        .find(|t| t["task_id"] == "implement")
        .expect("implement must appear");
    assert_eq!(implement["state"], "blocked");
    let blocked_by = implement["blocked_by"].as_array().unwrap();
    assert!(blocked_by.iter().any(|v| v == "write-tests"));
}

#[tokio::test]
async fn status_tasks_blocked_by_clears_after_completion() {
    let _g = isolate();
    let (_, tester, coder) = create_swarm_with_workers().await;

    SwarmAssignHandler
        .execute(json!({"agent_id": tester, "task_id": "write-tests"}))
        .await
        .unwrap();
    SwarmAssignHandler
        .execute(json!({
            "agent_id": coder,
            "task_id": "implement",
            "depends_on": ["write-tests"]
        }))
        .await
        .unwrap();
    SwarmHeartbeatHandler
        .execute(json!({"agent_id": tester, "status": "completed"}))
        .await
        .unwrap();

    let status = SwarmStatusHandler.execute(json!({})).await.unwrap();
    let tasks = status["tasks"].as_array().unwrap();
    let implement = tasks
        .iter()
        .find(|t| t["task_id"] == "implement")
        .expect("implement must appear");
    let blocked_by = implement["blocked_by"].as_array().unwrap();
    assert!(
        blocked_by.is_empty(),
        "blocked_by must be empty once dep completes"
    );
}
