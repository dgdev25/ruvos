//! Integration tests for the redb-backed `Store` and signed `.rvf` snapshots.

use ruvos_store::{AgentRecord, EventRecord, MessageRecord, MetricRecord, Store, TaskRecord};
use std::sync::Arc;
use std::thread;

fn tmp_store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.redb");
    let store = Store::open(path.to_str().unwrap()).unwrap();
    (dir, store)
}

#[test]
fn agents_put_get_list_roundtrip() {
    let (_d, s) = tmp_store();
    let a = AgentRecord::new("alpha", "coder");
    s.put_agent(&a).unwrap();
    let got = s.get_agent(&a.id).unwrap().unwrap();
    assert_eq!(got, a);
    assert_eq!(s.list_agents().unwrap().len(), 1);
    assert!(s.delete_agent(&a.id).unwrap());
    assert!(s.get_agent(&a.id).unwrap().is_none());
    assert!(!s.delete_agent(&a.id).unwrap());
}

#[test]
fn list_agents_by_status_filters() {
    let (_d, s) = tmp_store();
    let mut a = AgentRecord::new("alpha", "coder");
    a.status = "active".into();
    let mut b = AgentRecord::new("beta", "tester");
    b.status = "idle".into();
    s.put_agent(&a).unwrap();
    s.put_agent(&b).unwrap();
    let active = s.list_agents_by_status("active").unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].name, "alpha");
}

#[test]
fn claim_task_is_race_safe() {
    // A single Store instance shared across threads (redb takes a process
    // file lock, so only one handle may hold the db).
    let (_d, store) = tmp_store();
    let store = Arc::new(store);

    // Seed one pending task.
    let task = TaskRecord::new("build", serde_json::json!({}), 1);
    let task_id = task.id.clone();
    store.put_task(&task).unwrap();

    // Two threads race to claim the same task; exactly one must win.
    let s1 = Arc::clone(&store);
    let s2 = Arc::clone(&store);
    let id1 = task_id.clone();
    let id2 = task_id.clone();
    let h1 = thread::spawn(move || s1.claim_task(&id1, "agent-1").unwrap());
    let h2 = thread::spawn(move || s2.claim_task(&id2, "agent-2").unwrap());
    let r1 = h1.join().unwrap();
    let r2 = h2.join().unwrap();
    assert!(r1 ^ r2, "exactly one claim must succeed (got {r1}/{r2})");

    let claimed = store.get_task(&task_id).unwrap().unwrap();
    assert_eq!(claimed.status, "assigned");
    assert!(claimed.assigned_to.is_some());

    // tasks_by_agent reflects the winner.
    let winner = claimed.assigned_to.clone().unwrap();
    assert_eq!(store.tasks_by_agent(&winner).unwrap().len(), 1);
}

#[test]
fn pending_tasks_priority_ordered() {
    let (_d, s) = tmp_store();
    let low = TaskRecord::new("a", serde_json::json!({}), 0);
    let high = TaskRecord::new("b", serde_json::json!({}), 3);
    let mid = TaskRecord::new("c", serde_json::json!({}), 1);
    let mut done = TaskRecord::new("d", serde_json::json!({}), 2);
    done.status = "completed".into();
    s.put_task(&low).unwrap();
    s.put_task(&high).unwrap();
    s.put_task(&mid).unwrap();
    s.put_task(&done).unwrap();

    let pending = s.pending_tasks().unwrap();
    assert_eq!(pending.len(), 3, "completed task excluded");
    assert_eq!(pending[0].priority, 3);
    assert_eq!(pending[1].priority, 1);
    assert_eq!(pending[2].priority, 0);
}

#[test]
fn events_since_filters_by_timestamp() {
    let (_d, s) = tmp_store();
    let mut old = EventRecord::new("spawn", serde_json::json!({}));
    old.timestamp = 1000;
    let mut mid = EventRecord::new("edit", serde_json::json!({}));
    mid.timestamp = 2000;
    let mut new = EventRecord::new("done", serde_json::json!({}));
    new.timestamp = 3000;
    s.put_event(&old).unwrap();
    s.put_event(&new).unwrap();
    s.put_event(&mid).unwrap();

    let since = s.events_since(2000).unwrap();
    assert_eq!(since.len(), 2);
    // ascending time order from the index range scan
    assert_eq!(since[0].timestamp, 2000);
    assert_eq!(since[1].timestamp, 3000);

    // by_agent / by_type
    let mut tagged = EventRecord::new("edit", serde_json::json!({}));
    tagged.agent_id = Some("ag".into());
    s.put_event(&tagged).unwrap();
    assert_eq!(s.events_by_agent("ag", 10).unwrap().len(), 1);
    assert_eq!(s.events_by_type("edit", 10).unwrap().len(), 2);
}

#[test]
fn messages_between_unread_and_mark_read() {
    let (_d, s) = tmp_store();
    let m1 = MessageRecord::new("a", "b", "ping", serde_json::json!({"n": 1}));
    let m2 = MessageRecord::new("b", "a", "pong", serde_json::json!({"n": 2}));
    let other = MessageRecord::new("a", "c", "ping", serde_json::json!({}));
    s.put_message(&m1).unwrap();
    s.put_message(&m2).unwrap();
    s.put_message(&other).unwrap();

    let between = s.messages_between("a", "b", 10).unwrap();
    assert_eq!(between.len(), 2);

    let unread_b = s.unread_messages("b").unwrap();
    assert_eq!(unread_b.len(), 1);
    assert_eq!(unread_b[0].id, m1.id);

    s.mark_message_read(&m1.id).unwrap();
    assert!(s.unread_messages("b").unwrap().is_empty());
    // The other direction's message is still unread for "a".
    assert_eq!(s.unread_messages("a").unwrap().len(), 1);
}

#[test]
fn aggregated_metric_averages_window() {
    let (_d, s) = tmp_store();
    for (ts, v) in [(100, 10.0), (200, 20.0), (300, 30.0), (400, 99.0)] {
        let mut m = MetricRecord::new("latency", v, "ms");
        m.timestamp = ts;
        m.agent_id = Some("ag".into());
        s.put_metric(&m).unwrap();
    }
    // window [100, 300] -> avg(10,20,30) = 20
    let avg = s.aggregated_metric("latency", 100, 300).unwrap();
    assert!((avg - 20.0).abs() < 1e-9, "got {avg}");
    // empty window -> 0
    assert_eq!(s.aggregated_metric("latency", 500, 600).unwrap(), 0.0);
    assert_eq!(s.metrics_by_agent("ag", "latency").unwrap().len(), 4);
}

#[test]
fn snapshot_roundtrips_and_tamper_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    // Pin a deterministic signing key for the test.
    std::env::set_var("RUVOS_RVF_KEY", "test-signing-key-1234567890");

    let db_path = dir.path().join("a.redb");
    let s = Store::open(db_path.to_str().unwrap()).unwrap();
    let a = AgentRecord::new("alpha", "coder");
    s.put_agent(&a).unwrap();
    let task = TaskRecord::new("build", serde_json::json!({"x": 1}), 2);
    s.put_task(&task).unwrap();
    let mut ev = EventRecord::new("spawn", serde_json::json!({}));
    ev.timestamp = 1234;
    s.put_event(&ev).unwrap();
    let msg = MessageRecord::new("a", "b", "ping", serde_json::json!({}));
    s.put_message(&msg).unwrap();
    let met = MetricRecord::new("latency", 42.0, "ms");
    s.put_metric(&met).unwrap();

    let snap_path = dir.path().join("snap.rvf");
    let snap_path = snap_path.to_str().unwrap().to_string();
    s.snapshot_to_rvf(&snap_path).unwrap();

    // Restore into a fresh store and confirm every record came back.
    let db2 = dir.path().join("b.redb");
    let mut s2 = Store::open(db2.to_str().unwrap()).unwrap();
    s2.restore_from_rvf(&snap_path).unwrap();
    assert_eq!(s2.get_agent(&a.id).unwrap().unwrap(), a);
    assert_eq!(s2.get_task(&task.id).unwrap().unwrap(), task);
    assert_eq!(s2.events_since(0).unwrap().len(), 1);
    assert_eq!(s2.messages_between("a", "b", 10).unwrap().len(), 1);
    assert_eq!(s2.aggregated_metric("latency", 0, i64::MAX).unwrap(), 42.0);

    // Tamper the snapshot payload on disk -> restore must fail.
    let raw = std::fs::read_to_string(&snap_path).unwrap();
    let tampered = raw.replace("alpha", "evil");
    assert_ne!(raw, tampered, "test setup: payload must contain the token");
    std::fs::write(&snap_path, tampered).unwrap();
    let db3 = dir.path().join("c.redb");
    let mut s3 = Store::open(db3.to_str().unwrap()).unwrap();
    assert!(
        s3.restore_from_rvf(&snap_path).is_err(),
        "tampered snapshot must fail verification"
    );

    std::env::remove_var("RUVOS_RVF_KEY");
}
