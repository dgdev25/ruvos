//! redb-backed [`Store`] — the live, queryable persistence layer.
//!
//! Each record type lives in its own primary table keyed by `id` and holding
//! the JSON-serialized record. A small secondary index table keeps an
//! efficient ordering for `events_since` (keyed by a zero-padded
//! `timestamp:id` composite so a `range` scan is monotonic).
//!
//! All write paths use a single redb write transaction, which gives us the
//! race-safety guarantee for [`Store::claim_task`]: the read-check-write of a
//! task's `assigned_to` field happens inside one serialized transaction, so two
//! concurrent claimers cannot both succeed.

use crate::records::{
    now_secs, AgentRecord, EventRecord, MessageRecord, MetricRecord, StoreSnapshot, TaskRecord,
};
use crate::snapshot;
use redb::{Database, ReadableTable, TableDefinition};
use std::sync::Arc;

type Result<T> = anyhow::Result<T>;

// Primary tables: id -> JSON record.
const AGENTS: TableDefinition<&str, &str> = TableDefinition::new("agents");
const TASKS: TableDefinition<&str, &str> = TableDefinition::new("tasks");
const EVENTS: TableDefinition<&str, &str> = TableDefinition::new("events");
const MESSAGES: TableDefinition<&str, &str> = TableDefinition::new("messages");
const METRICS: TableDefinition<&str, &str> = TableDefinition::new("metrics");

// Secondary index: zero-padded "timestamp:id" -> event id, for efficient
// time-range scans in `events_since`.
const EVENT_BY_TIME: TableDefinition<&str, &str> = TableDefinition::new("event_by_time");

/// Zero-pad a (possibly negative) UNIX-second timestamp into a
/// lexicographically sortable key prefix. Offsetting by a large bias keeps
/// negative timestamps ordered correctly.
fn time_key(ts: i64, id: &str) -> String {
    let biased = (ts as i128) + 1_000_000_000_000_i128;
    format!("{biased:020}:{id}")
}

/// The redb-backed store with signed `.rvf` snapshot provenance.
pub struct Store {
    db: Arc<Database>,
}

impl Store {
    /// Open (or create) the redb file at `db_path`, ensuring all tables exist.
    pub fn open(db_path: &str) -> Result<Self> {
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let db = Database::create(db_path)?;
        // Materialize tables so empty reads don't fail.
        let txn = db.begin_write()?;
        {
            let _ = txn.open_table(AGENTS)?;
            let _ = txn.open_table(TASKS)?;
            let _ = txn.open_table(EVENTS)?;
            let _ = txn.open_table(MESSAGES)?;
            let _ = txn.open_table(METRICS)?;
            let _ = txn.open_table(EVENT_BY_TIME)?;
        }
        txn.commit()?;
        Ok(Self { db: Arc::new(db) })
    }

    // ---- generic helpers ------------------------------------------------

    fn put_json<T: serde::Serialize>(
        &self,
        table: TableDefinition<&str, &str>,
        id: &str,
        value: &T,
    ) -> Result<()> {
        let json = serde_json::to_string(value)?;
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(table)?;
            t.insert(id, json.as_str())?;
        }
        txn.commit()?;
        Ok(())
    }

    fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        table: TableDefinition<&str, &str>,
        id: &str,
    ) -> Result<Option<T>> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(table)?;
        match t.get(id)? {
            Some(v) => Ok(Some(serde_json::from_str(v.value())?)),
            None => Ok(None),
        }
    }

    fn scan_all<T: serde::de::DeserializeOwned>(
        &self,
        table: TableDefinition<&str, &str>,
    ) -> Result<Vec<T>> {
        let txn = self.db.begin_read()?;
        let t = txn.open_table(table)?;
        let mut out = Vec::new();
        for item in t.iter()? {
            let (_, v) = item?;
            out.push(serde_json::from_str(v.value())?);
        }
        Ok(out)
    }

    // ---- agents ---------------------------------------------------------

    /// Insert or update an agent.
    pub fn put_agent(&self, a: &AgentRecord) -> Result<()> {
        self.put_json(AGENTS, &a.id, a)
    }

    /// Fetch an agent by id.
    pub fn get_agent(&self, id: &str) -> Result<Option<AgentRecord>> {
        self.get_json(AGENTS, id)
    }

    /// Delete an agent; returns whether a row was removed.
    pub fn delete_agent(&self, id: &str) -> Result<bool> {
        let txn = self.db.begin_write()?;
        let removed;
        {
            let mut t = txn.open_table(AGENTS)?;
            removed = t.remove(id)?.is_some();
        }
        txn.commit()?;
        Ok(removed)
    }

    /// List all agents.
    pub fn list_agents(&self) -> Result<Vec<AgentRecord>> {
        self.scan_all(AGENTS)
    }

    /// List agents filtered by status.
    pub fn list_agents_by_status(&self, status: &str) -> Result<Vec<AgentRecord>> {
        Ok(self
            .list_agents()?
            .into_iter()
            .filter(|a| a.status == status)
            .collect())
    }

    // ---- tasks ----------------------------------------------------------

    /// Insert or update a task.
    pub fn put_task(&self, t: &TaskRecord) -> Result<()> {
        self.put_json(TASKS, &t.id, t)
    }

    /// Fetch a task by id.
    pub fn get_task(&self, id: &str) -> Result<Option<TaskRecord>> {
        self.get_json(TASKS, id)
    }

    /// Pending tasks, highest priority first (ties broken by creation time).
    pub fn pending_tasks(&self) -> Result<Vec<TaskRecord>> {
        let mut tasks: Vec<TaskRecord> = self
            .scan_all::<TaskRecord>(TASKS)?
            .into_iter()
            .filter(|t| t.status == "pending")
            .collect();
        tasks.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then(a.created_at.cmp(&b.created_at))
        });
        Ok(tasks)
    }

    /// Tasks currently assigned to `agent_id`.
    pub fn tasks_by_agent(&self, agent_id: &str) -> Result<Vec<TaskRecord>> {
        Ok(self
            .scan_all::<TaskRecord>(TASKS)?
            .into_iter()
            .filter(|t| t.assigned_to.as_deref() == Some(agent_id))
            .collect())
    }

    /// Atomically claim a pending task for `agent_id`.
    ///
    /// Race-safety: the read of the task, the check that it is still
    /// claimable, and the write of `assigned_to`/`status` all occur inside a
    /// **single** redb write transaction. redb serializes write transactions,
    /// so if two agents call `claim_task` for the same task concurrently, one
    /// transaction commits the claim and the other observes `status !=
    /// "pending"` (or a non-null `assigned_to`) and returns `false`.
    pub fn claim_task(&self, task_id: &str, agent_id: &str) -> Result<bool> {
        let txn = self.db.begin_write()?;
        let claimed;
        {
            let mut t = txn.open_table(TASKS)?;
            let current: Option<TaskRecord> = match t.get(task_id)? {
                Some(v) => Some(serde_json::from_str(v.value())?),
                None => None,
            };
            match current {
                Some(mut task) if task.status == "pending" && task.assigned_to.is_none() => {
                    task.assigned_to = Some(agent_id.to_string());
                    task.status = "assigned".to_string();
                    task.updated_at = now_secs();
                    let json = serde_json::to_string(&task)?;
                    t.insert(task_id, json.as_str())?;
                    claimed = true;
                }
                _ => claimed = false,
            }
        }
        txn.commit()?;
        Ok(claimed)
    }

    // ---- events ---------------------------------------------------------

    /// Append an event to the audit log (also indexed by time).
    pub fn put_event(&self, e: &EventRecord) -> Result<()> {
        let json = serde_json::to_string(e)?;
        let tkey = time_key(e.timestamp, &e.id);
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(EVENTS)?;
            t.insert(e.id.as_str(), json.as_str())?;
            let mut idx = txn.open_table(EVENT_BY_TIME)?;
            idx.insert(tkey.as_str(), e.id.as_str())?;
        }
        txn.commit()?;
        Ok(())
    }

    /// Events at or after `ts_secs`, in ascending time order. Uses the time
    /// index `range` for an efficient scan rather than a full table sweep.
    pub fn events_since(&self, ts_secs: i64) -> Result<Vec<EventRecord>> {
        let lower = time_key(ts_secs, "");
        let txn = self.db.begin_read()?;
        let idx = txn.open_table(EVENT_BY_TIME)?;
        let events = txn.open_table(EVENTS)?;
        let mut out = Vec::new();
        for item in idx.range(lower.as_str()..)? {
            let (_, id) = item?;
            if let Some(v) = events.get(id.value())? {
                out.push(serde_json::from_str(v.value())?);
            }
        }
        Ok(out)
    }

    /// Most recent events for an agent (descending time), capped at `limit`.
    pub fn events_by_agent(&self, agent_id: &str, limit: usize) -> Result<Vec<EventRecord>> {
        let mut events: Vec<EventRecord> = self
            .scan_all::<EventRecord>(EVENTS)?
            .into_iter()
            .filter(|e| e.agent_id.as_deref() == Some(agent_id))
            .collect();
        events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        events.truncate(limit);
        Ok(events)
    }

    /// Most recent events of a given type (descending time), capped at `limit`.
    pub fn events_by_type(&self, event_type: &str, limit: usize) -> Result<Vec<EventRecord>> {
        let mut events: Vec<EventRecord> = self
            .scan_all::<EventRecord>(EVENTS)?
            .into_iter()
            .filter(|e| e.event_type == event_type)
            .collect();
        events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        events.truncate(limit);
        Ok(events)
    }

    // ---- messages -------------------------------------------------------

    /// Insert or update a message.
    pub fn put_message(&self, m: &MessageRecord) -> Result<()> {
        self.put_json(MESSAGES, &m.id, m)
    }

    /// Messages exchanged between two agents (either direction), oldest first,
    /// capped at `limit`.
    pub fn messages_between(&self, a: &str, b: &str, limit: usize) -> Result<Vec<MessageRecord>> {
        let mut msgs: Vec<MessageRecord> = self
            .scan_all::<MessageRecord>(MESSAGES)?
            .into_iter()
            .filter(|m| {
                (m.from_agent == a && m.to_agent == b) || (m.from_agent == b && m.to_agent == a)
            })
            .collect();
        msgs.sort_by(|x, y| x.created_at.cmp(&y.created_at));
        msgs.truncate(limit);
        Ok(msgs)
    }

    /// Unread messages addressed to `agent_id`, oldest first.
    pub fn unread_messages(&self, agent_id: &str) -> Result<Vec<MessageRecord>> {
        let mut msgs: Vec<MessageRecord> = self
            .scan_all::<MessageRecord>(MESSAGES)?
            .into_iter()
            .filter(|m| m.to_agent == agent_id && !m.read)
            .collect();
        msgs.sort_by(|x, y| x.created_at.cmp(&y.created_at));
        Ok(msgs)
    }

    /// Mark a message as read (no-op if it does not exist).
    pub fn mark_message_read(&self, id: &str) -> Result<()> {
        let txn = self.db.begin_write()?;
        {
            let mut t = txn.open_table(MESSAGES)?;
            // Read and decode, releasing the access guard before the mutation.
            let updated = match t.get(id)? {
                Some(v) => {
                    let mut m: MessageRecord = serde_json::from_str(v.value())?;
                    m.read = true;
                    m.read_at = Some(now_secs());
                    Some(serde_json::to_string(&m)?)
                }
                None => None,
            };
            if let Some(json) = updated {
                t.insert(id, json.as_str())?;
            }
        }
        txn.commit()?;
        Ok(())
    }

    // ---- metrics --------------------------------------------------------

    /// Insert a metric data point.
    pub fn put_metric(&self, m: &MetricRecord) -> Result<()> {
        self.put_json(METRICS, &m.id, m)
    }

    /// Metrics for an agent of a given type.
    pub fn metrics_by_agent(&self, agent_id: &str, metric_type: &str) -> Result<Vec<MetricRecord>> {
        Ok(self
            .scan_all::<MetricRecord>(METRICS)?
            .into_iter()
            .filter(|m| m.agent_id.as_deref() == Some(agent_id) && m.metric_type == metric_type)
            .collect())
    }

    /// Average value of a metric type within the `[start, end]` window
    /// (inclusive). Returns `0.0` if there are no samples in range.
    pub fn aggregated_metric(&self, metric_type: &str, start: i64, end: i64) -> Result<f64> {
        let vals: Vec<f64> = self
            .scan_all::<MetricRecord>(METRICS)?
            .into_iter()
            .filter(|m| m.metric_type == metric_type && m.timestamp >= start && m.timestamp <= end)
            .map(|m| m.value)
            .collect();
        if vals.is_empty() {
            return Ok(0.0);
        }
        Ok(vals.iter().sum::<f64>() / vals.len() as f64)
    }

    // ---- provenance -----------------------------------------------------

    /// Collect every record into a [`StoreSnapshot`] (stable id order).
    fn snapshot(&self) -> Result<StoreSnapshot> {
        let mut agents: Vec<AgentRecord> = self.scan_all(AGENTS)?;
        agents.sort_by(|a, b| a.id.cmp(&b.id));
        let mut tasks: Vec<TaskRecord> = self.scan_all(TASKS)?;
        tasks.sort_by(|a, b| a.id.cmp(&b.id));
        let mut events: Vec<EventRecord> = self.scan_all(EVENTS)?;
        events.sort_by(|a, b| a.id.cmp(&b.id));
        let mut messages: Vec<MessageRecord> = self.scan_all(MESSAGES)?;
        messages.sort_by(|a, b| a.id.cmp(&b.id));
        let mut metrics: Vec<MetricRecord> = self.scan_all(METRICS)?;
        metrics.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(StoreSnapshot {
            agents,
            tasks,
            events,
            messages,
            metrics,
        })
    }

    /// Write a signed `.rvf` snapshot of the entire store to `path`.
    pub fn snapshot_to_rvf(&self, path: &str) -> Result<()> {
        let snap = self.snapshot()?;
        let container = snapshot::seal(snap);
        snapshot::write_to(&container, path)
    }

    /// Restore the store from a signed `.rvf` snapshot, replacing all records.
    /// Fails (and leaves the store untouched) if the snapshot fails witness
    /// verification.
    pub fn restore_from_rvf(&mut self, path: &str) -> Result<()> {
        let snap = snapshot::read_from(path)?;
        let txn = self.db.begin_write()?;
        {
            // Clear every table, then repopulate from the snapshot.
            clear_table(&txn, AGENTS)?;
            clear_table(&txn, TASKS)?;
            clear_table(&txn, EVENTS)?;
            clear_table(&txn, MESSAGES)?;
            clear_table(&txn, METRICS)?;
            clear_table(&txn, EVENT_BY_TIME)?;

            let mut t = txn.open_table(AGENTS)?;
            for a in &snap.agents {
                t.insert(a.id.as_str(), serde_json::to_string(a)?.as_str())?;
            }
            let mut t = txn.open_table(TASKS)?;
            for x in &snap.tasks {
                t.insert(x.id.as_str(), serde_json::to_string(x)?.as_str())?;
            }
            let mut t = txn.open_table(EVENTS)?;
            let mut idx = txn.open_table(EVENT_BY_TIME)?;
            for e in &snap.events {
                t.insert(e.id.as_str(), serde_json::to_string(e)?.as_str())?;
                idx.insert(time_key(e.timestamp, &e.id).as_str(), e.id.as_str())?;
            }
            let mut t = txn.open_table(MESSAGES)?;
            for m in &snap.messages {
                t.insert(m.id.as_str(), serde_json::to_string(m)?.as_str())?;
            }
            let mut t = txn.open_table(METRICS)?;
            for m in &snap.metrics {
                t.insert(m.id.as_str(), serde_json::to_string(m)?.as_str())?;
            }
        }
        txn.commit()?;
        Ok(())
    }
}

/// Remove every entry from a table within an existing write transaction.
fn clear_table(txn: &redb::WriteTransaction, def: TableDefinition<&str, &str>) -> Result<()> {
    let keys: Vec<String> = {
        let t = txn.open_table(def)?;
        let mut ks = Vec::new();
        for item in t.iter()? {
            let (k, _) = item?;
            ks.push(k.value().to_string());
        }
        ks
    };
    let mut t = txn.open_table(def)?;
    for k in keys {
        t.remove(k.as_str())?;
    }
    Ok(())
}
