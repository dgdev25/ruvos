//! SQLite backend implementation for native platforms

use crate::{models::*, Storage, StorageError, Transaction as TransactionTrait};
use async_trait::async_trait;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::Arc;
use tracing::{debug, info};

/// SQLite storage implementation backed by a single Mutex-guarded connection.
/// WAL mode makes concurrent readers + one writer safe without a pool.
pub struct SqliteStorage {
    conn: Arc<Mutex<Connection>>,
    path: String,
}

impl SqliteStorage {
    /// Create new SQLite storage instance
    pub async fn new(path: &str) -> Result<Self, StorageError> {
        let raw = Connection::open(path).map_err(|e| StorageError::Database(e.to_string()))?;

        let storage = Self {
            conn: Arc::new(Mutex::new(raw)),
            path: path.to_string(),
        };

        storage.init_schema_with_migrations().await?;
        storage.configure_sqlite().await?;

        info!("SQLite storage initialized at: {}", path);
        Ok(storage)
    }

    /// Create SQLite storage from an existing connection (for testing)
    #[cfg(test)]
    pub async fn from_conn(conn: Connection) -> Result<Self, StorageError> {
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            path: ":memory:".to_string(),
        })
    }

    /// Borrow the mutex-guarded connection
    fn get_conn(&self) -> parking_lot::MutexGuard<'_, Connection> {
        self.conn.lock()
    }

    /// Get connection (for testing)
    #[cfg(test)]
    pub fn get_conn_test(&self) -> parking_lot::MutexGuard<'_, Connection> {
        self.get_conn()
    }

    /// Execute a blocking database operation using spawn_blocking
    async fn exec_blocking<F, R>(&self, operation: F) -> Result<R, StorageError>
    where
        F: FnOnce(&Connection) -> Result<R, rusqlite::Error> + Send + 'static,
        R: Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let guard = conn.lock();
            operation(&guard).map_err(|e| StorageError::Database(e.to_string()))
        })
        .await
        .map_err(|e| StorageError::Other(format!("Join error: {}", e)))?
    }

    /// Execute a blocking database operation with retry logic for SQLITE_BUSY
    async fn exec_blocking_with_retry<F, R>(&self, operation: F) -> Result<R, StorageError>
    where
        F: Fn(&Connection) -> Result<R, rusqlite::Error> + Send + Clone + 'static,
        R: Send + 'static,
    {
        const MAX_RETRIES: u32 = 10;
        const BASE_DELAY_MS: u64 = 5;

        let mut retries = 0;
        loop {
            let conn = Arc::clone(&self.conn);
            let op = operation.clone();
            let result = tokio::task::spawn_blocking(move || {
                let guard = conn.lock();
                op(&guard).map_err(|e| StorageError::Database(e.to_string()))
            })
            .await
            .map_err(|e| StorageError::Other(format!("Join error: {}", e)))?;

            match result {
                Ok(val) => return Ok(val),
                Err(StorageError::Database(ref err_str))
                    if (err_str.contains("database is locked")
                        || err_str.contains("database table is locked")
                        || err_str.contains("SQLITE_BUSY"))
                        && retries < MAX_RETRIES =>
                {
                    retries += 1;
                    let base_delay = BASE_DELAY_MS * (1 << retries.min(5));
                    let jitter = fastrand::u64(0..base_delay / 2);
                    let delay = base_delay + jitter;
                    debug!(
                        "Database locked, retry {} of {} with {}ms delay",
                        retries, MAX_RETRIES, delay
                    );
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Synchronous retry helper (used by `with_retry`)
    async fn with_retry<F, T>(&self, operation: F) -> Result<T, StorageError>
    where
        F: Fn(&Connection) -> Result<T, rusqlite::Error> + Send + Clone + 'static,
        T: Send + 'static,
    {
        self.exec_blocking_with_retry(operation).await
    }

    /// Initialize database schema using migration system
    async fn init_schema_with_migrations(&self) -> Result<(), StorageError> {
        self.exec_blocking(move |conn| {
            let manager = crate::migrations::MigrationManager::new();
            manager.migrate(conn).map_err(|e| {
                rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
                    Some(e.to_string()),
                )
            })?;
            debug!("Schema initialized via migrations");
            Ok(())
        })
        .await
    }

    /// Configure SQLite settings after schema initialization
    async fn configure_sqlite(&self) -> Result<(), StorageError> {
        self.exec_blocking(move |conn| {
            conn.execute_batch(
                r#"
                PRAGMA journal_mode = WAL;
                PRAGMA synchronous = NORMAL;
                PRAGMA busy_timeout = 30000;
                PRAGMA foreign_keys = ON;
                PRAGMA wal_autocheckpoint = 1000;
                PRAGMA temp_store = MEMORY;
                PRAGMA mmap_size = 268435456;
                "#,
            )?;
            debug!("SQLite configuration complete: WAL mode, busy_timeout=30s");
            Ok(())
        })
        .await
    }

    /// Deserialize rows, logging (but not failing on) bad JSON
    fn deserialize_rows<T, I>(&self, rows: I) -> Result<Vec<T>, StorageError>
    where
        T: serde::de::DeserializeOwned,
        I: Iterator<Item = Result<String, rusqlite::Error>>,
    {
        let mut results = Vec::new();
        let mut errors = Vec::new();

        for (idx, row_result) in rows.enumerate() {
            match row_result {
                Ok(json) => match serde_json::from_str::<T>(&json) {
                    Ok(item) => results.push(item),
                    Err(e) => {
                        errors.push(format!("Row {}: JSON parse error: {}", idx, e));
                        debug!("Failed to parse JSON at row {}: {}", idx, e);
                    }
                },
                Err(e) => {
                    errors.push(format!("Row {}: Database error: {}", idx, e));
                    debug!("Failed to read row {}: {}", idx, e);
                }
            }
        }

        if !errors.is_empty() {
            debug!("Encountered {} errors during deserialization", errors.len());
        }

        Ok(results)
    }
}

#[async_trait]
impl Storage for SqliteStorage {
    type Error = StorageError;

    // Agent operations
    async fn store_agent(&self, agent: &AgentModel) -> Result<(), Self::Error> {
        let json = serde_json::to_string(agent)?;
        let capabilities_json = serde_json::to_string(&agent.capabilities)?;
        let metadata_json = serde_json::to_string(&agent.metadata)?;

        let agent_id = agent.id.clone();
        let agent_name = agent.name.clone();
        let agent_type = agent.agent_type.clone();
        let status = agent.status.to_string();
        let heartbeat = agent.heartbeat.timestamp();
        let created_at = agent.created_at.timestamp();
        let updated_at = agent.updated_at.timestamp();

        self.exec_blocking_with_retry(move |conn| {
            conn.execute(
                "INSERT INTO agents (id, name, agent_type, status, capabilities, metadata, heartbeat, created_at, updated_at, data) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    &agent_id, &agent_name, &agent_type, &status,
                    &capabilities_json, &metadata_json,
                    heartbeat, created_at, updated_at, &json
                ],
            )
        })
        .await?;

        debug!("Stored agent: {}", agent.id);
        Ok(())
    }

    async fn get_agent(&self, id: &str) -> Result<Option<AgentModel>, Self::Error> {
        let id = id.to_string();
        let result = self
            .exec_blocking(move |conn| {
                conn.query_row(
                    "SELECT data FROM agents WHERE id = ?1",
                    params![id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
            })
            .await?;

        match result {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    async fn update_agent(&self, agent: &AgentModel) -> Result<(), Self::Error> {
        let json = serde_json::to_string(agent)?;
        let capabilities_json = serde_json::to_string(&agent.capabilities)?;
        let metadata_json = serde_json::to_string(&agent.metadata)?;

        let agent_id = agent.id.clone();
        let agent_name = agent.name.clone();
        let agent_type = agent.agent_type.clone();
        let status = agent.status.to_string();
        let heartbeat = agent.heartbeat.timestamp();
        let updated_at = agent.updated_at.timestamp();

        let rows = self
            .exec_blocking_with_retry(move |conn| {
                conn.execute(
                    "UPDATE agents \
                     SET name = ?2, agent_type = ?3, status = ?4, capabilities = ?5, \
                         metadata = ?6, heartbeat = ?7, updated_at = ?8, data = ?9 \
                     WHERE id = ?1",
                    params![
                        &agent_id,
                        &agent_name,
                        &agent_type,
                        &status,
                        &capabilities_json,
                        &metadata_json,
                        heartbeat,
                        updated_at,
                        &json
                    ],
                )
            })
            .await?;

        if rows == 0 {
            return Err(StorageError::NotFound(format!(
                "Agent {} not found",
                agent.id
            )));
        }

        debug!("Updated agent: {}", agent.id);
        Ok(())
    }

    async fn delete_agent(&self, id: &str) -> Result<(), Self::Error> {
        let id = id.to_string();
        let id_for_debug = id.clone();
        let rows = self
            .exec_blocking_with_retry(move |conn| {
                conn.execute("DELETE FROM agents WHERE id = ?1", params![&id])
            })
            .await?;

        if rows > 0 {
            debug!("Deleted agent: {}", id_for_debug);
        } else {
            debug!("Agent {} not found, delete is idempotent", id_for_debug);
        }
        Ok(())
    }

    async fn list_agents(&self) -> Result<Vec<AgentModel>, Self::Error> {
        let json_results = self
            .exec_blocking(move |conn| {
                let mut stmt = conn.prepare("SELECT data FROM agents ORDER BY created_at DESC")?;
                let agents: Result<Vec<String>, _> =
                    stmt.query_map([], |row| row.get::<_, String>(0))?.collect();
                agents
            })
            .await?;

        let agents = json_results
            .into_iter()
            .filter_map(|json| serde_json::from_str(&json).ok())
            .collect();

        Ok(agents)
    }

    async fn list_agents_by_status(&self, status: &str) -> Result<Vec<AgentModel>, Self::Error> {
        let conn = self.get_conn();

        let mut stmt = conn
            .prepare("SELECT data FROM agents WHERE status = ?1 ORDER BY created_at DESC")
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let rows = stmt
            .query_map(params![status], |row| row.get::<_, String>(0))
            .map_err(|e| StorageError::Database(e.to_string()))?;

        self.deserialize_rows(rows)
    }

    // Task operations
    async fn store_task(&self, task: &TaskModel) -> Result<(), Self::Error> {
        let conn = self.get_conn();
        let json = serde_json::to_string(task)?;

        conn.execute(
            "INSERT INTO tasks (id, task_type, priority, status, assigned_to, payload, \
                                created_at, updated_at, data) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                task.id,
                task.task_type,
                task.priority as i32,
                serde_json::to_value(&task.status)?.as_str().unwrap(),
                task.assigned_to,
                serde_json::to_string(&task.payload)?,
                task.created_at.timestamp(),
                task.updated_at.timestamp(),
                json
            ],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;

        debug!("Stored task: {}", task.id);
        Ok(())
    }

    async fn get_task(&self, id: &str) -> Result<Option<TaskModel>, Self::Error> {
        let conn = self.get_conn();

        let result = conn
            .query_row("SELECT data FROM tasks WHERE id = ?1", params![id], |row| {
                row.get::<_, String>(0)
            })
            .optional()
            .map_err(|e| StorageError::Database(e.to_string()))?;

        match result {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    async fn update_task(&self, task: &TaskModel) -> Result<(), Self::Error> {
        let conn = self.get_conn();
        let json = serde_json::to_string(task)?;

        let rows = conn
            .execute(
                "UPDATE tasks \
             SET task_type = ?2, priority = ?3, status = ?4, assigned_to = ?5, \
                 payload = ?6, updated_at = ?7, data = ?8 \
             WHERE id = ?1",
                params![
                    task.id,
                    task.task_type,
                    task.priority as i32,
                    serde_json::to_value(&task.status)?.as_str().unwrap(),
                    task.assigned_to,
                    serde_json::to_string(&task.payload)?,
                    task.updated_at.timestamp(),
                    json
                ],
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;

        if rows == 0 {
            return Err(StorageError::NotFound(format!(
                "Task {} not found",
                task.id
            )));
        }

        debug!("Updated task: {}", task.id);
        Ok(())
    }

    async fn get_pending_tasks(&self) -> Result<Vec<TaskModel>, Self::Error> {
        let conn = self.get_conn();

        let mut stmt = conn
            .prepare(
                "SELECT data FROM tasks \
                 WHERE status = 'pending' \
                 ORDER BY priority DESC, created_at ASC",
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let tasks = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| StorageError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|json| serde_json::from_str(&json).ok())
            .collect();

        Ok(tasks)
    }

    async fn get_tasks_by_agent(&self, agent_id: &str) -> Result<Vec<TaskModel>, Self::Error> {
        let conn = self.get_conn();

        let mut stmt = conn
            .prepare(
                "SELECT data FROM tasks \
                 WHERE assigned_to = ?1 \
                 ORDER BY priority DESC, created_at ASC",
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let tasks = stmt
            .query_map(params![agent_id], |row| row.get::<_, String>(0))
            .map_err(|e| StorageError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|json| serde_json::from_str(&json).ok())
            .collect();

        Ok(tasks)
    }

    async fn claim_task(&self, task_id: &str, agent_id: &str) -> Result<bool, Self::Error> {
        let task_id = task_id.to_owned();
        let agent_id = agent_id.to_owned();

        self.with_retry(move |conn| {
            let timestamp = chrono::Utc::now().timestamp();
            conn.execute(
                "UPDATE tasks \
                 SET assigned_to = ?2, status = 'assigned', updated_at = ?3 \
                 WHERE id = ?1 AND status = 'pending'",
                params![&task_id, &agent_id, timestamp],
            )
        })
        .await
        .map(|rows| rows > 0)
    }

    // Event operations
    async fn store_event(&self, event: &EventModel) -> Result<(), Self::Error> {
        let conn = self.get_conn();
        let json = serde_json::to_string(event)?;

        conn.execute(
            "INSERT INTO events (id, event_type, agent_id, task_id, payload, metadata, \
                                 timestamp, sequence, data) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                event.id,
                event.event_type,
                event.agent_id,
                event.task_id,
                serde_json::to_string(&event.payload)?,
                serde_json::to_string(&event.metadata)?,
                event.timestamp.timestamp(),
                event.sequence as i64,
                json
            ],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;

        debug!("Stored event: {}", event.id);
        Ok(())
    }

    async fn get_events_by_agent(
        &self,
        agent_id: &str,
        limit: usize,
    ) -> Result<Vec<EventModel>, Self::Error> {
        let conn = self.get_conn();

        let mut stmt = conn
            .prepare(
                "SELECT data FROM events \
                 WHERE agent_id = ?1 \
                 ORDER BY timestamp DESC, id DESC \
                 LIMIT ?2",
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let events = stmt
            .query_map(params![agent_id, limit], |row| row.get::<_, String>(0))
            .map_err(|e| StorageError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|json| serde_json::from_str(&json).ok())
            .collect();

        Ok(events)
    }

    async fn get_events_by_type(
        &self,
        event_type: &str,
        limit: usize,
    ) -> Result<Vec<EventModel>, Self::Error> {
        let conn = self.get_conn();

        let mut stmt = conn
            .prepare(
                "SELECT data FROM events \
                 WHERE event_type = ?1 \
                 ORDER BY timestamp DESC, id DESC \
                 LIMIT ?2",
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let events = stmt
            .query_map(params![event_type, limit], |row| row.get::<_, String>(0))
            .map_err(|e| StorageError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|json| serde_json::from_str(&json).ok())
            .collect();

        Ok(events)
    }

    async fn get_events_since(&self, timestamp: i64) -> Result<Vec<EventModel>, Self::Error> {
        let conn = self.get_conn();

        let mut stmt = conn
            .prepare(
                "SELECT data FROM events \
                 WHERE timestamp >= ?1 \
                 ORDER BY timestamp ASC, id ASC",
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let events = stmt
            .query_map(params![timestamp], |row| row.get::<_, String>(0))
            .map_err(|e| StorageError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|json| serde_json::from_str(&json).ok())
            .collect();

        Ok(events)
    }

    // Message operations
    async fn store_message(&self, message: &MessageModel) -> Result<(), Self::Error> {
        let conn = self.get_conn();
        let json = serde_json::to_string(message)?;

        conn.execute(
            "INSERT INTO messages (id, from_agent, to_agent, message_type, content, \
                                   priority, read, created_at, data) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                message.id,
                message.from_agent,
                message.to_agent,
                message.message_type,
                serde_json::to_string(&message.content)?,
                serde_json::to_string(&message.priority)?,
                message.read as i32,
                message.created_at.timestamp(),
                json
            ],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;

        debug!("Stored message: {}", message.id);
        Ok(())
    }

    async fn get_messages_between(
        &self,
        agent1: &str,
        agent2: &str,
        limit: usize,
    ) -> Result<Vec<MessageModel>, Self::Error> {
        let conn = self.get_conn();

        let mut stmt = conn
            .prepare(
                "SELECT data FROM messages \
                 WHERE (from_agent = ?1 AND to_agent = ?2) OR (from_agent = ?2 AND to_agent = ?1) \
                 ORDER BY created_at DESC \
                 LIMIT ?3",
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let messages = stmt
            .query_map(params![agent1, agent2, limit], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| StorageError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|json| serde_json::from_str(&json).ok())
            .collect();

        Ok(messages)
    }

    async fn get_unread_messages(&self, agent_id: &str) -> Result<Vec<MessageModel>, Self::Error> {
        let conn = self.get_conn();

        let mut stmt = conn
            .prepare(
                "SELECT data FROM messages \
                 WHERE to_agent = ?1 AND read = 0 \
                 ORDER BY created_at ASC",
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let messages = stmt
            .query_map(params![agent_id], |row| row.get::<_, String>(0))
            .map_err(|e| StorageError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|json| serde_json::from_str(&json).ok())
            .collect();

        Ok(messages)
    }

    async fn mark_message_read(&self, message_id: &str) -> Result<(), Self::Error> {
        let conn = self.get_conn();

        let rows = conn
            .execute(
                "UPDATE messages SET read = 1, read_at = ?2 WHERE id = ?1",
                params![message_id, chrono::Utc::now().timestamp()],
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;

        if rows == 0 {
            return Err(StorageError::NotFound(format!(
                "Message {} not found",
                message_id
            )));
        }

        Ok(())
    }

    // Metric operations
    async fn store_metric(&self, metric: &MetricModel) -> Result<(), Self::Error> {
        let conn = self.get_conn();
        let json = serde_json::to_string(metric)?;

        conn.execute(
            "INSERT INTO metrics (id, metric_type, agent_id, value, unit, tags, timestamp, data) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                metric.id,
                metric.metric_type,
                metric.agent_id,
                metric.value,
                metric.unit,
                serde_json::to_string(&metric.tags)?,
                metric.timestamp.timestamp(),
                json
            ],
        )
        .map_err(|e| StorageError::Database(e.to_string()))?;

        debug!("Stored metric: {}", metric.id);
        Ok(())
    }

    async fn get_metrics_by_agent(
        &self,
        agent_id: &str,
        metric_type: &str,
    ) -> Result<Vec<MetricModel>, Self::Error> {
        let conn = self.get_conn();

        let mut stmt = conn
            .prepare(
                "SELECT data FROM metrics \
                 WHERE agent_id = ?1 AND metric_type = ?2 \
                 ORDER BY timestamp DESC, id DESC",
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let metrics = stmt
            .query_map(params![agent_id, metric_type], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|e| StorageError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .filter_map(|json| serde_json::from_str(&json).ok())
            .collect();

        Ok(metrics)
    }

    async fn get_aggregated_metrics(
        &self,
        metric_type: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<MetricModel>, Self::Error> {
        let conn = self.get_conn();

        let mut stmt = conn
            .prepare(
                "SELECT metric_type, agent_id, AVG(value) as value, unit, \
                        MIN(timestamp) as timestamp, COUNT(*) as count \
                 FROM metrics \
                 WHERE metric_type = ?1 AND timestamp >= ?2 AND timestamp <= ?3 \
                 GROUP BY metric_type, agent_id",
            )
            .map_err(|e| StorageError::Database(e.to_string()))?;

        let metrics = stmt
            .query_map(params![metric_type, start_time, end_time], |row| {
                let mut metric = MetricModel::new(
                    row.get::<_, String>(0)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, String>(3)?,
                );
                metric.agent_id = row.get::<_, Option<String>>(1)?;
                metric
                    .tags
                    .insert("count".to_string(), row.get::<_, i64>(5)?.to_string());
                Ok(metric)
            })
            .map_err(|e| StorageError::Database(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(metrics)
    }

    // Transaction support
    async fn begin_transaction(&self) -> Result<Box<dyn TransactionTrait>, Self::Error> {
        // For simplicity with a single Mutex<Connection>, we use a lightweight
        // transaction that holds the guard across calls via a channel.
        // The caller gets an opaque handle; commit/rollback sends the verdict.
        let conn = Arc::clone(&self.conn);
        let tx = SqliteTransaction::new(conn)?;
        Ok(Box::new(tx))
    }

    // Maintenance operations
    async fn vacuum(&self) -> Result<(), Self::Error> {
        let conn = self.get_conn();
        conn.execute("VACUUM", [])
            .map_err(|e| StorageError::Database(e.to_string()))?;
        info!("Database vacuumed");
        Ok(())
    }

    async fn checkpoint(&self) -> Result<(), Self::Error> {
        #[cfg(test)]
        {
            debug!("Skipping checkpoint in test mode");
            return Ok(());
        }

        #[cfg(not(test))]
        {
            self.exec_blocking(move |conn| {
                conn.execute("PRAGMA wal_checkpoint(TRUNCATE)", [])?;
                info!("Database checkpoint completed");
                Ok(())
            })
            .await
        }
    }

    async fn get_storage_size(&self) -> Result<u64, Self::Error> {
        let metadata =
            std::fs::metadata(&self.path).map_err(|e| StorageError::Other(e.to_string()))?;
        Ok(metadata.len())
    }
}

/// SQLite transaction wrapper.
///
/// Because we use a single `Arc<Mutex<Connection>>`, we begin a transaction by
/// locking the mutex on construction and holding the guard inside an `Option`.
/// On commit/rollback we drop the guard, releasing the mutex.
struct SqliteTransaction {
    conn: Arc<Mutex<Connection>>,
    committed: Arc<Mutex<bool>>,
    begun: bool,
}

impl SqliteTransaction {
    fn new(conn: Arc<Mutex<Connection>>) -> Result<Self, StorageError> {
        {
            let guard = conn.lock();
            guard.execute("BEGIN DEFERRED", []).map_err(|e| {
                debug!("Failed to begin transaction: {}", e);
                StorageError::Transaction(format!("Failed to begin transaction: {}", e))
            })?;
        }

        Ok(Self {
            conn,
            committed: Arc::new(Mutex::new(false)),
            begun: true,
        })
    }
}

impl Drop for SqliteTransaction {
    fn drop(&mut self) {
        let committed = *self.committed.lock();
        if self.begun && !committed {
            let guard = self.conn.lock();
            let _ = guard.execute("ROLLBACK", []);
            debug!("Transaction automatically rolled back on drop");
        }
    }
}

#[async_trait]
impl TransactionTrait for SqliteTransaction {
    async fn commit(mut self: Box<Self>) -> Result<(), StorageError> {
        let mut committed = self.committed.lock();
        if *committed {
            return Err(StorageError::Transaction(
                "Transaction already committed".to_string(),
            ));
        }

        let guard = self.conn.lock();
        guard.execute("COMMIT", []).map_err(|e| {
            StorageError::Transaction(format!("Failed to commit transaction: {}", e))
        })?;
        *committed = true;
        self.begun = false;
        Ok(())
    }

    async fn rollback(mut self: Box<Self>) -> Result<(), StorageError> {
        let committed = *self.committed.lock();
        if committed {
            return Err(StorageError::Transaction(
                "Cannot rollback committed transaction".to_string(),
            ));
        }

        let guard = self.conn.lock();
        guard.execute("ROLLBACK", []).map_err(|e| {
            StorageError::Transaction(format!("Failed to rollback transaction: {}", e))
        })?;
        self.begun = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_sqlite_storage() {
        let temp_file = NamedTempFile::new().unwrap();
        let storage = SqliteStorage::new(temp_file.path().to_str().unwrap())
            .await
            .unwrap();

        // Test agent operations
        let agent = AgentModel::new(
            "test-agent".to_string(),
            "worker".to_string(),
            vec!["compute".to_string()],
        );

        storage.store_agent(&agent).await.unwrap();
        let retrieved = storage.get_agent(&agent.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "test-agent");

        // Test task operations
        let task = TaskModel::new(
            "process".to_string(),
            serde_json::json!({"data": "test"}),
            TaskPriority::High,
        );

        storage.store_task(&task).await.unwrap();
        let pending = storage.get_pending_tasks().await.unwrap();
        assert_eq!(pending.len(), 1);
    }
}
