//! Mapping helpers between the MCP-facing agent view and the redb-backed
//! [`ruvos_store`] records.
//!
//! `agent.*` persists into `ruvos-store` (redb + signed `.rvf` snapshots). The
//! MCP surface still speaks in terms of archetype / traits / model / prompt /
//! artifact / messages, so this module owns the (de)serialization of those
//! fields into a store [`AgentRecord`]'s `agent_type` / `capabilities` /
//! `metadata`, plus the construction of `EventRecord`s and the message-count
//! query used by `agent.status` and `agent.message`.

use crate::store::store;
use crate::{Result, RuvosError};
use ruvos_store::{AgentRecord, EventRecord, MessageRecord};
use serde_json::{json, Value};

/// The synthetic "from" agent used for system-originated messages so that the
/// store's `messages_between(SYSTEM, agent_id)` query yields the agent's inbox.
pub const SYSTEM_AGENT: &str = "system";

/// Metadata keys stored on the redb [`AgentRecord`].
mod meta {
    pub const MODEL: &str = "model";
    pub const PROMPT: &str = "prompt";
    pub const ARTIFACT_PATH: &str = "artifact_path";
    pub const ARTIFACT_BYTES: &str = "artifact_bytes";
    pub const RESULT: &str = "result";
    pub const CREATED_AT: &str = "created_at";
}

/// Wrap a store error into a [`RuvosError`].
fn store_err(ctx: &str, e: anyhow::Error) -> RuvosError {
    RuvosError::InternalError(format!("{ctx}: {e}"))
}

/// Build a fully-populated store [`AgentRecord`] for a freshly spawned agent.
#[allow(clippy::too_many_arguments)]
pub fn build_agent_record(
    agent_id: &str,
    archetype: &str,
    traits: &[String],
    model: &str,
    prompt: &str,
    status: &str,
    artifact_path: &str,
    artifact_bytes: u64,
    result: &str,
    created_at: &str,
) -> AgentRecord {
    let mut rec = AgentRecord::new(agent_id.to_string(), archetype.to_string());
    rec.id = agent_id.to_string();
    rec.status = status.to_string();
    rec.capabilities = traits.to_vec();
    rec.metadata.insert(meta::MODEL.to_string(), json!(model));
    rec.metadata.insert(meta::PROMPT.to_string(), json!(prompt));
    rec.metadata
        .insert(meta::ARTIFACT_PATH.to_string(), json!(artifact_path));
    rec.metadata
        .insert(meta::ARTIFACT_BYTES.to_string(), json!(artifact_bytes));
    rec.metadata.insert(meta::RESULT.to_string(), json!(result));
    rec.metadata
        .insert(meta::CREATED_AT.to_string(), json!(created_at));
    rec
}

/// Read a string field from an agent's metadata, defaulting to empty.
fn meta_str(rec: &AgentRecord, key: &str) -> String {
    rec.metadata
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

/// Persist a new agent record + an `agent.spawned` event.
pub fn persist_spawn(rec: &AgentRecord) -> Result<()> {
    let s = store();
    s.put_agent(rec).map_err(|e| store_err("put_agent", e))?;

    let mut ev = EventRecord::new(
        "agent.spawned",
        json!({
            "archetype": rec.agent_type,
            "model": meta_str(rec, meta::MODEL),
            "artifact_path": meta_str(rec, meta::ARTIFACT_PATH),
        }),
    );
    ev.agent_id = Some(rec.id.clone());
    s.put_event(&ev).map_err(|e| store_err("put_event", e))?;
    Ok(())
}

/// Count messages addressed to an agent (system → agent inbox).
pub fn message_count(agent_id: &str) -> u64 {
    let s = store();
    s.messages_between(SYSTEM_AGENT, agent_id, usize::MAX)
        .map(|m| m.len() as u64)
        .unwrap_or(0)
}

/// Fetch one agent as a `agent.status` JSON view, or `None` if absent.
pub fn status_view(agent_id: &str, transport_live: bool) -> Result<Option<Value>> {
    let s = store();
    let rec = s
        .get_agent(agent_id)
        .map_err(|e| store_err("get_agent", e))?;
    Ok(rec.map(|a| {
        let count = message_count(&a.id);
        json!({
            "found": true,
            "agent_id": a.id,
            "archetype": a.agent_type,
            "status": a.status,
            "artifact_path": meta_str(&a, meta::ARTIFACT_PATH),
            "message_count": count,
            "result": meta_str(&a, meta::RESULT),
            "transport_live": transport_live
        })
    }))
}

/// List all agents as the `agent.status` summary view.
pub fn list_view() -> Result<Vec<Value>> {
    let s = store();
    let agents = s.list_agents().map_err(|e| store_err("list_agents", e))?;
    Ok(agents
        .into_iter()
        .map(|a| {
            let count = message_count(&a.id);
            json!({
                "agent_id": a.id,
                "archetype": a.agent_type,
                "status": a.status,
                "created_at": meta_str(&a, meta::CREATED_AT),
                "message_count": count
            })
        })
        .collect())
}

/// Append a message to an agent (if it exists). Returns
/// `Some((message_id, new_count))` on delivery, `None` if the agent is unknown.
pub fn append_message(agent_id: &str, content: &str) -> Result<Option<(String, u64)>> {
    let s = store();
    let exists = s
        .get_agent(agent_id)
        .map_err(|e| store_err("get_agent", e))?
        .is_some();
    if !exists {
        return Ok(None);
    }
    let msg = MessageRecord::new(SYSTEM_AGENT, agent_id, "agent.message", json!(content));
    let msg_id = msg.id.clone();
    s.put_message(&msg)
        .map_err(|e| store_err("put_message", e))?;

    let mut ev = EventRecord::new("agent.message", json!({ "content": content }));
    ev.agent_id = Some(agent_id.to_string());
    s.put_event(&ev).map_err(|e| store_err("put_event", e))?;

    Ok(Some((msg_id, message_count(agent_id))))
}
