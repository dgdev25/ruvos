//! Lightweight async task queue for off-MCP-path hook execution (ADR-029).
//!
//! Implements the behavioral contract from ADR-029 without pulling in the
//! apalis crate dependency. When apalis stabilises on stable Rust, `TaskQueue`
//! can be swapped for an apalis backend behind the same public API without
//! touching callers.
//!
//! # Behavioural contract
//! - `enqueue()` — fire-and-forget: the hook runs in a detached `tokio::spawn`
//!   task; the caller gets a `task_id` immediately and the MCP response is not
//!   blocked.
//! - `enqueue_and_wait()` — synchronous path used by `flush_hooks: true`;
//!   awaits completion and returns the result.  Intended for tests and
//!   debugging only.
//!
//! # Retry
//! Each task is retried up to `MAX_RETRIES` times with exponential back-off
//! (50 ms base, capped at 5 s).  After exhausting retries the failure is
//! written to the ruvos event log via `publish_event`.

use crate::runtime::{publish_event, RuntimeEvent};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::oneshot;
use uuid::Uuid;

const MAX_RETRIES: u32 = 3;

/// A unit of work that the queue can execute.  The `work` field is a
/// `Box<dyn Fn() -> ...>` equivalent, captured as a future factory so each
/// retry gets a fresh future.
pub struct QueuedTask {
    pub id: String,
    pub label: String,
    pub work:
        Arc<dyn Fn() -> futures::future::BoxFuture<'static, Result<Value, String>> + Send + Sync>,
}

impl QueuedTask {
    pub fn new<F, Fut>(label: impl Into<String>, f: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<Value, String>> + Send + 'static,
    {
        QueuedTask {
            id: Uuid::new_v4().to_string(),
            label: label.into(),
            work: Arc::new(move || Box::pin(f())),
        }
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

/// The task queue.  Currently stateless (no in-memory queue needed because
/// each task spawns its own tokio task).  A persistent SQLite backend can be
/// added here transparently.
pub struct TaskQueue;

impl TaskQueue {
    pub fn new() -> Self {
        TaskQueue
    }

    /// Fire-and-forget: spawn the task and return its `task_id` immediately.
    pub fn enqueue(&self, task: QueuedTask) -> String {
        let id = task.id.clone();
        let label = task.label.clone();
        tokio::spawn(async move {
            match run_with_retry(&task).await {
                Ok(_) => publish_event(RuntimeEvent {
                    kind: "task_queue.completed".into(),
                    payload: json!({"task_id": task.id, "label": label}),
                    agent_id: None,
                    task_id: Some(task.id.clone()),
                }),
                Err(e) => publish_event(RuntimeEvent {
                    kind: "task_queue.failed".into(),
                    payload: json!({"task_id": task.id, "label": label, "error": e}),
                    agent_id: None,
                    task_id: Some(task.id.clone()),
                }),
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
}

impl Default for TaskQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn enqueue_and_wait_returns_result() {
        let q = TaskQueue::new();
        let task = QueuedTask::new("test", || async { Ok(json!({"done": true})) });
        let result = q.enqueue_and_wait(task).await.unwrap();
        assert_eq!(result["done"], true);
    }

    #[tokio::test]
    async fn enqueue_and_wait_retries_on_failure() {
        use std::sync::atomic::{AtomicU32, Ordering};
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
        let q = TaskQueue::new();
        let task = QueuedTask::new("ff", || async { Ok(json!({})) });
        let id = q.enqueue(task);
        assert!(!id.is_empty());
    }
}
