//! Durable async task queue for off-MCP-path hook execution (ADR-029).
//!
//! Every fire-and-forget task is persisted to SQLite (`$RUVOS_HOME/tasks.db`)
//! *before* it is spawned, so work enqueued by `hooks_post` survives a process
//! crash: `recover_pending()` re-executes any task that never completed. This
//! is the durable-queue guarantee that replaces the v3 in-process daemon
//! (Windows persistence bug #1766 class).
//!
//! # Behavioural contract
//! - `enqueue()` — fire-and-forget: the row is committed as `pending`, the
//!   work runs in a detached `tokio::spawn`; the caller gets a `task_id`
//!   immediately and the MCP response is not blocked. On success the row is
//!   deleted; on exhausted retries it is marked `failed` with the error.
//! - `enqueue_and_wait()` — synchronous path used by `flush_hooks: true`;
//!   awaits completion and returns the result. Not persisted (the caller is
//!   waiting, so a crash is the caller's failure too).
//! - `recover_pending(executor)` — called once at server startup; re-runs
//!   every `pending` row through `executor(task_type, payload)`.
//!
//! # Retry
//! Each task is retried up to `MAX_RETRIES` times with exponential back-off
//! (50 ms base, capped at 5 s). After exhausting retries the failure is
//! written to the ruvos event log via `publish_event` and the row is marked
//! `failed` (kept for inspection, pruned after 7 days).

use crate::runtime::{publish_event, RuntimeEvent};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::oneshot;
use uuid::Uuid;

const MAX_RETRIES: u32 = 3;
const FAILED_RETENTION_SECS: i64 = 7 * 86_400;

/// A unit of work that the queue can execute. `task_type` + `payload` are the
/// durable description (enough to re-execute after a crash); `work` is the
/// in-process future factory used for the first, live execution.
pub struct QueuedTask {
    pub id: String,
    pub label: String,
    pub task_type: String,
    pub payload: Value,
    pub work:
        Arc<dyn Fn() -> futures::future::BoxFuture<'static, Result<Value, String>> + Send + Sync>,
}

impl QueuedTask {
    /// A durable task: `task_type` + `payload` are persisted so the work can
    /// be re-executed by `recover_pending` after a crash.
    pub fn durable<F, Fut>(
        label: impl Into<String>,
        task_type: impl Into<String>,
        payload: Value,
        f: F,
    ) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<Value, String>> + Send + 'static,
    {
        QueuedTask {
            id: Uuid::new_v4().to_string(),
            label: label.into(),
            task_type: task_type.into(),
            payload,
            work: Arc::new(move || Box::pin(f())),
        }
    }

    /// An ephemeral task (no recovery payload). Used by `enqueue_and_wait`.
    pub fn new<F, Fut>(label: impl Into<String>, f: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<Value, String>> + Send + 'static,
    {
        Self::durable(label, "ephemeral", Value::Null, f)
    }
}

/// Execute `task.work` with retry + exponential back-off.
async fn run_with_retry(task: &QueuedTask) -> Result<Value, String> {
    let mut delay = std::time::Duration::from_millis(50);
    for attempt in 0..MAX_RETRIES {
        match (task.work)().await {
            Ok(v) => return Ok(v),
            Err(e) if attempt + 1 == MAX_RETRIES => return Err(e),
            Err(_) => {
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(std::time::Duration::from_secs(5));
            }
        }
    }
    Err(format!("task {} exhausted retries", task.id))
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// SQLite-backed store for queued tasks.
struct TaskStore {
    path: PathBuf,
}

impl TaskStore {
    fn open_default() -> Self {
        TaskStore {
            path: crate::paths::data_root().join("tasks.db"),
        }
    }

    fn conn(&self) -> rusqlite::Result<Connection> {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(&self.path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tasks (
                id          TEXT PRIMARY KEY,
                task_type   TEXT NOT NULL,
                label       TEXT NOT NULL,
                payload     TEXT NOT NULL,
                status      TEXT NOT NULL DEFAULT 'pending',
                attempts    INTEGER NOT NULL DEFAULT 0,
                last_error  TEXT,
                created_at  INTEGER NOT NULL,
                updated_at  INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);",
        )?;
        Ok(conn)
    }

    fn insert_pending(&self, task: &QueuedTask) -> rusqlite::Result<()> {
        let now = now_secs();
        self.conn()?.execute(
            "INSERT INTO tasks (id, task_type, label, payload, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?5)",
            params![
                task.id,
                task.task_type,
                task.label,
                task.payload.to_string(),
                now
            ],
        )?;
        Ok(())
    }

    fn mark_completed(&self, id: &str) -> rusqlite::Result<()> {
        self.conn()?
            .execute("DELETE FROM tasks WHERE id = ?1", params![id])?;
        Ok(())
    }

    fn mark_failed(&self, id: &str, error: &str) -> rusqlite::Result<()> {
        self.conn()?.execute(
            "UPDATE tasks SET status = 'failed', last_error = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, error, now_secs()],
        )?;
        Ok(())
    }

    fn pending(&self) -> rusqlite::Result<Vec<(String, String, String, Value)>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, task_type, label, payload FROM tasks WHERE status = 'pending'
             ORDER BY created_at",
        )?;
        let rows = stmt.query_map([], |row| {
            let payload_raw: String = row.get(3)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                serde_json::from_str(&payload_raw).unwrap_or(Value::Null),
            ))
        })?;
        rows.collect()
    }

    fn prune_old_failures(&self) -> rusqlite::Result<()> {
        self.conn()?.execute(
            "DELETE FROM tasks WHERE status = 'failed' AND updated_at < ?1",
            params![now_secs() - FAILED_RETENTION_SECS],
        )?;
        Ok(())
    }
}

/// The durable task queue. Disk (SQLite) is the source of truth for what work
/// is owed; tokio tasks are just the live execution vehicle.
pub struct TaskQueue {
    store: TaskStore,
}

impl TaskQueue {
    pub fn new() -> Self {
        TaskQueue {
            store: TaskStore::open_default(),
        }
    }

    /// Fire-and-forget: persist as `pending`, spawn the work, and return the
    /// `task_id` immediately. The row is deleted on success and marked
    /// `failed` after exhausted retries — a crash in between leaves a
    /// `pending` row for `recover_pending` to pick up.
    pub fn enqueue(&self, task: QueuedTask) -> String {
        let id = task.id.clone();
        let label = task.label.clone();
        if let Err(e) = self.store.insert_pending(&task) {
            // Persistence failure must not block the hook path — execute
            // anyway, but record the degraded durability.
            publish_event(RuntimeEvent {
                kind: "task_queue.persist_failed".into(),
                payload: json!({"task_id": id, "label": label, "error": e.to_string()}),
                agent_id: None,
                task_id: Some(id.clone()),
            });
        }
        let store_path = self.store.path.clone();
        tokio::spawn(async move {
            let store = TaskStore { path: store_path };
            match run_with_retry(&task).await {
                Ok(_) => {
                    let _ = store.mark_completed(&task.id);
                    publish_event(RuntimeEvent {
                        kind: "task_queue.completed".into(),
                        payload: json!({"task_id": task.id, "label": label}),
                        agent_id: None,
                        task_id: Some(task.id.clone()),
                    });
                }
                Err(e) => {
                    let _ = store.mark_failed(&task.id, &e);
                    publish_event(RuntimeEvent {
                        kind: "task_queue.failed".into(),
                        payload: json!({"task_id": task.id, "label": label, "error": e}),
                        agent_id: None,
                        task_id: Some(task.id.clone()),
                    });
                }
            }
        });
        id
    }

    /// Synchronous path: await completion and return the result.
    /// Used when `flush_hooks: true` is set on `hooks_post`.
    pub async fn enqueue_and_wait(&self, task: QueuedTask) -> Result<Value, String> {
        let (tx, rx) = oneshot::channel::<Result<Value, String>>();
        tokio::spawn(async move {
            let result = run_with_retry(&task).await;
            let _ = tx.send(result);
        });
        rx.await
            .unwrap_or_else(|_| Err("task channel dropped".into()))
    }

    /// Re-execute every task left `pending` by a previous process. Called once
    /// at server startup. `executor` maps a persisted `(task_type, payload)`
    /// back to live work; unknown task types are marked failed.
    pub async fn recover_pending<F>(&self, executor: F) -> usize
    where
        F: Fn(&str, Value) -> futures::future::BoxFuture<'static, Result<Value, String>>,
    {
        let _ = self.store.prune_old_failures();
        let pending = match self.store.pending() {
            Ok(rows) => rows,
            Err(_) => return 0,
        };
        let mut recovered = 0usize;
        for (id, task_type, label, payload) in pending {
            let result = executor(&task_type, payload).await;
            match result {
                Ok(_) => {
                    let _ = self.store.mark_completed(&id);
                    recovered += 1;
                    publish_event(RuntimeEvent {
                        kind: "task_queue.recovered".into(),
                        payload: json!({"task_id": id, "label": label}),
                        agent_id: None,
                        task_id: Some(id.clone()),
                    });
                }
                Err(e) => {
                    let _ = self.store.mark_failed(&id, &e);
                    publish_event(RuntimeEvent {
                        kind: "task_queue.recovery_failed".into(),
                        payload: json!({"task_id": id, "label": label, "error": e}),
                        agent_id: None,
                        task_id: Some(id.clone()),
                    });
                }
            }
        }
        recovered
    }
}

impl Default for TaskQueue {
    fn default() -> Self {
        Self::new()
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

    #[tokio::test]
    async fn enqueue_and_wait_returns_result() {
        let _g = isolate();
        let q = TaskQueue::new();
        let task = QueuedTask::new("test", || async { Ok(json!({"done": true})) });
        let result = q.enqueue_and_wait(task).await.unwrap();
        assert_eq!(result["done"], true);
    }

    #[tokio::test]
    async fn enqueue_and_wait_retries_on_failure() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let _g = isolate();
        let calls = Arc::new(AtomicU32::new(0));
        let c = calls.clone();
        let q = TaskQueue::new();
        let task = QueuedTask::new("retry-test", move || {
            let c = c.clone();
            async move {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err("transient".into())
                } else {
                    Ok(json!({"retried": n}))
                }
            }
        });
        let result = q.enqueue_and_wait(task).await.unwrap();
        assert!(result["retried"].as_u64().unwrap() >= 2);
        assert!(calls.load(Ordering::SeqCst) >= 3);
    }

    #[tokio::test]
    async fn enqueue_fire_and_forget_returns_id() {
        let _g = isolate();
        let q = TaskQueue::new();
        let task = QueuedTask::new("ff", || async { Ok(json!({})) });
        let id = q.enqueue(task);
        assert!(!id.is_empty());
    }

    #[tokio::test]
    async fn enqueued_task_is_persisted_before_execution() {
        let _g = isolate();
        let store = TaskStore::open_default();
        let task = QueuedTask::durable(
            "durable-test",
            "hooks_post",
            json!({"kind": "task"}),
            || async {
                // Never resolves within the test window — simulates a crash
                // before completion.
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                Ok(json!({}))
            },
        );
        let q = TaskQueue::new();
        let id = q.enqueue(task);

        let pending = store.pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0, id);
        assert_eq!(pending[0].1, "hooks_post");
        assert_eq!(pending[0].3, json!({"kind": "task"}));
    }

    #[tokio::test]
    async fn recover_pending_reexecutes_and_clears() {
        let _g = isolate();
        let store = TaskStore::open_default();
        // Simulate a crashed prior process: a pending row with no live task.
        let orphan =
            QueuedTask::durable("orphan", "hooks_post", json!({"kind": "edit"}), || async {
                Ok(json!({}))
            });
        store.insert_pending(&orphan).unwrap();

        let q = TaskQueue::new();
        let recovered = q
            .recover_pending(|task_type, payload| {
                let t = task_type.to_string();
                Box::pin(async move {
                    assert_eq!(t, "hooks_post");
                    assert_eq!(payload, json!({"kind": "edit"}));
                    Ok(json!({"replayed": true}))
                })
            })
            .await;

        assert_eq!(recovered, 1);
        assert!(store.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn failed_recovery_marks_row_failed() {
        let _g = isolate();
        let store = TaskStore::open_default();
        let orphan = QueuedTask::durable("orphan", "unknown_type", json!({}), || async {
            Ok(json!({}))
        });
        store.insert_pending(&orphan).unwrap();

        let q = TaskQueue::new();
        let recovered = q
            .recover_pending(|_, _| Box::pin(async { Err("unknown task type".to_string()) }))
            .await;

        assert_eq!(recovered, 0);
        assert!(store.pending().unwrap().is_empty());
    }

    #[tokio::test]
    async fn completed_task_row_is_deleted() {
        let _g = isolate();
        let store = TaskStore::open_default();
        let q = TaskQueue::new();
        let task = QueuedTask::durable("done", "hooks_post", json!({}), || async {
            Ok(json!({"ok": true}))
        });
        q.enqueue(task);
        // Wait for the detached task to complete and clean up its row.
        for _ in 0..50 {
            if store.pending().unwrap().is_empty() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("task row was not cleaned up after completion");
    }
}
