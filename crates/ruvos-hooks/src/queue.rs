use crate::types::*;
use anyhow::Result;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

pub struct HookQueue {
    conn: Connection,
}

impl HookQueue {
    pub fn new(db_path: &str) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        Self::init_schema(&conn)?;
        Ok(HookQueue { conn })
    }

    fn init_schema(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS hook_events (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                phase TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                payload TEXT NOT NULL,
                status TEXT NOT NULL
            )",
        )?;
        Ok(())
    }

    pub fn enqueue(
        &self,
        kind: HookKind,
        phase: HookPhase,
        payload: serde_json::Value,
    ) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let timestamp = Utc::now().to_rfc3339();
        let payload_str = serde_json::to_string(&payload)?;

        self.conn.execute(
            "INSERT INTO hook_events (id, kind, phase, timestamp, payload, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &id,
                kind.as_str(),
                phase.as_str(),
                &timestamp,
                &payload_str,
                "pending"
            ],
        )?;

        Ok(id)
    }

    pub fn dequeue(&self) -> Result<Option<HookEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, kind, phase, timestamp, payload, status FROM hook_events WHERE status = 'pending' ORDER BY timestamp LIMIT 1"
        )?;

        let event = stmt
            .query_row([], |row| {
                let id = row.get::<_, String>(0)?;
                let kind_str = row.get::<_, String>(1)?;
                let phase_str = row.get::<_, String>(2)?;
                let timestamp_str = row.get::<_, String>(3)?;
                let payload_str = row.get::<_, String>(4)?;
                let status_str = row.get::<_, String>(5)?;

                let kind = match kind_str.as_str() {
                    "task" => HookKind::Task,
                    "edit" => HookKind::Edit,
                    "command" => HookKind::Command,
                    "session" => HookKind::Session,
                    _ => HookKind::Task,
                };

                let phase = match phase_str.as_str() {
                    "pre" => HookPhase::Pre,
                    "post" => HookPhase::Post,
                    _ => HookPhase::Pre,
                };

                let status = match status_str.as_str() {
                    "pending" => EventStatus::Pending,
                    "processing" => EventStatus::Processing,
                    "completed" => EventStatus::Completed,
                    "failed" => EventStatus::Failed,
                    _ => EventStatus::Pending,
                };

                Ok(HookEvent {
                    id,
                    kind,
                    phase,
                    timestamp: chrono::DateTime::parse_from_rfc3339(&timestamp_str)
                        .map_err(|e| {
                            rusqlite::Error::InvalidParameterName(format!(
                                "invalid timestamp: {}",
                                e
                            ))
                        })?
                        .with_timezone(&Utc),
                    payload: serde_json::from_str(&payload_str).map_err(|e| {
                        rusqlite::Error::InvalidParameterName(format!("invalid json: {}", e))
                    })?,
                    status,
                })
            })
            .optional()?;

        Ok(event)
    }

    pub fn mark_completed(&self, event_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE hook_events SET status = 'completed' WHERE id = ?1",
            params![event_id],
        )?;
        Ok(())
    }

    pub fn mark_failed(&self, event_id: &str, _reason: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE hook_events SET status = 'failed' WHERE id = ?1",
            params![event_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_enqueue_and_dequeue() {
        let queue = HookQueue::new(":memory:").expect("create queue");
        let payload = json!({"task": "test"});

        let id = queue
            .enqueue(HookKind::Task, HookPhase::Pre, payload.clone())
            .expect("enqueue event");

        assert!(!id.is_empty());

        let event = queue
            .dequeue()
            .expect("dequeue event")
            .expect("event exists");
        assert_eq!(event.id, id);
        assert_eq!(event.kind, HookKind::Task);
        assert_eq!(event.phase, HookPhase::Pre);
    }
}
