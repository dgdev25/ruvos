//! Swarm domain control-plane handlers (13): create, status, assign, heartbeat, message, complete, fail, health, rebalance, join, leave, report, metrics.
//!
//! This is the first control-plane layer for multi-agent swarm orchestration.
//! It persists one active swarm state per data root so higher-level commands can
//! coordinate agent pools, roles, and topology explicitly.

use super::handler::{ExecuteFuture, ToolHandler};
use crate::constants::{
    SWARM_CREATE_DEFAULT_MAX_AGENTS, SWARM_HYBRID_MAX_AGENTS_THRESHOLD,
    SWARM_HYBRID_MEMBER_THRESHOLD,
};
use crate::runtime::{publish_event, RuntimeEvent};
use crate::tools::agent_store;
use crate::{relay, swarm, Result, RuvosError};
use serde_json::{json, Value};
use uuid::Uuid;

fn load_active_swarm(requested_swarm_id: Option<&str>) -> Result<swarm::SwarmState> {
    let state = swarm::current()
        .ok_or_else(|| RuvosError::HandlerError("no active swarm found".to_string()))?;
    if let Some(requested_swarm_id) = requested_swarm_id {
        if state.id != requested_swarm_id {
            return Err(RuvosError::InvalidParams(format!(
                "swarm_id '{requested_swarm_id}' does not match active swarm '{}'",
                state.id
            )));
        }
    }
    Ok(state)
}

fn swarm_filter_events<'a>(
    events: &'a [ruvos_store::EventRecord],
    swarm_id: &str,
) -> Vec<&'a ruvos_store::EventRecord> {
    events
        .iter()
        .filter(|event| {
            event
                .payload
                .get("swarm_id")
                .and_then(|v| v.as_str())
                .map(|id| id == swarm_id)
                .unwrap_or(false)
        })
        .collect()
}

fn swarm_event_counts(events: &[&ruvos_store::EventRecord]) -> serde_json::Value {
    let mut counts = std::collections::BTreeMap::<String, u64>::new();
    for event in events {
        if event.event_type.starts_with("swarm.") {
            *counts.entry(event.event_type.clone()).or_default() += 1;
        }
    }
    json!(counts)
}

fn swarm_task_counts(state: &swarm::SwarmState) -> (u64, u64, u64) {
    let assigned = state
        .members
        .iter()
        .map(|member| member.assigned_tasks.len() as u64)
        .sum::<u64>();
    let active = state
        .members
        .iter()
        .filter(|member| member.state == "active" || member.state == "assigned")
        .count() as u64;
    let left = state
        .members
        .iter()
        .filter(|member| member.state == "left")
        .count() as u64;
    (assigned, active, left)
}

fn swarm_metrics(
    state: &swarm::SwarmState,
    events: &[&ruvos_store::EventRecord],
) -> serde_json::Value {
    let now = chrono::Utc::now();
    let live_members = live_member_indices(state, now).len() as u64;
    let stale_members = state.members.len() as u64 - live_members;
    let (assigned_tasks, active_agents, left_members) = swarm_task_counts(state);
    let event_counts = swarm_event_counts(events);
    let completed = events
        .iter()
        .filter(|event| event.event_type == "swarm.completed")
        .count() as u64;
    let failed = events
        .iter()
        .filter(|event| event.event_type == "swarm.failed")
        .count() as u64;
    let joined = events
        .iter()
        .filter(|event| event.event_type == "swarm.joined")
        .count() as u64;
    let left = events
        .iter()
        .filter(|event| event.event_type == "swarm.left")
        .count() as u64;
    let messages = events
        .iter()
        .filter(|event| event.event_type == "swarm.message")
        .count() as u64;
    let health_score = if state.members.is_empty() {
        0.0
    } else {
        live_members as f64 / state.members.len() as f64
    };

    json!({
        "swarm_id": state.id,
        "member_count": state.members.len() as u64,
        "live_members": live_members,
        "stale_members": stale_members,
        "active_agents": active_agents,
        "left_members": left_members,
        "assigned_tasks": assigned_tasks,
        "completed_events": completed,
        "failed_events": failed,
        "joined_events": joined,
        "left_events": left,
        "message_events": messages,
        "health_score": health_score,
        "event_counts": event_counts,
    })
}

pub struct TopologyDecision {
    pub topology: String,
    pub reason: String,
    pub source: String,
}

fn swarm_text(params: &Value) -> String {
    [
        params.get("objective").and_then(|v| v.as_str()),
        params.get("task").and_then(|v| v.as_str()),
        params.get("goal").and_then(|v| v.as_str()),
        params.get("summary").and_then(|v| v.as_str()),
        params.get("reason").and_then(|v| v.as_str()),
        params.get("description").and_then(|v| v.as_str()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ")
    .to_lowercase()
}

pub fn recommend_topology(
    params: &Value,
    member_count: usize,
    max_agents: u32,
) -> TopologyDecision {
    if let Some(topology) = params.get("topology").and_then(|v| v.as_str()) {
        return TopologyDecision {
            topology: topology.to_string(),
            reason: "explicit".to_string(),
            source: "explicit".to_string(),
        };
    }

    let objective = params
        .get("objective")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if let Some((topology, reason)) = swarm::learned_topology(objective, member_count, max_agents) {
        return TopologyDecision {
            topology,
            reason,
            source: "learned".to_string(),
        };
    }

    let text = swarm_text(params);
    let mesh_keywords = [
        "broadcast",
        "peer",
        "mesh",
        "collaborat",
        "discover",
        "fan out",
        "parallel",
        "multi-terminal",
        "multi terminal",
        "swarm",
    ];
    if mesh_keywords.iter().any(|needle| text.contains(needle)) {
        return TopologyDecision {
            topology: "mesh".to_string(),
            reason: "inferred from collaboration keywords".to_string(),
            source: "inferred".to_string(),
        };
    }

    let adaptive_keywords = ["adaptive", "self-organ", "self organiz", "dynamic"];
    if adaptive_keywords.iter().any(|needle| text.contains(needle)) {
        return TopologyDecision {
            topology: "adaptive".to_string(),
            reason: "inferred from adaptive keywords".to_string(),
            source: "inferred".to_string(),
        };
    }

    let hybrid_keywords = [
        "parallel",
        "distributed",
        "recovery",
        "rebalance",
        "stale",
        "multi-step",
        "multi step",
        "many agents",
        "scale",
    ];
    if hybrid_keywords.iter().any(|needle| text.contains(needle))
        || member_count > SWARM_HYBRID_MEMBER_THRESHOLD
        || max_agents > SWARM_HYBRID_MAX_AGENTS_THRESHOLD
    {
        return TopologyDecision {
            topology: "hybrid".to_string(),
            reason: "inferred from task size or recovery keywords".to_string(),
            source: "inferred".to_string(),
        };
    }

    TopologyDecision {
        topology: "hierarchical".to_string(),
        reason: "defaulted to hierarchical".to_string(),
        source: "inferred".to_string(),
    }
}

fn infer_topology(params: &Value, member_count: usize, max_agents: u32) -> TopologyDecision {
    recommend_topology(params, member_count, max_agents)
}

fn member_exists(state: &swarm::SwarmState, agent_id: &str) -> bool {
    state
        .members
        .iter()
        .any(|member| member.agent_id == agent_id)
}

fn message_targets(params: &Value, state: &swarm::SwarmState, sender: &str) -> Result<Vec<String>> {
    if params
        .get("broadcast")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Ok(state
            .members
            .iter()
            .map(|member| member.agent_id.clone())
            .filter(|agent_id| agent_id != sender)
            .collect());
    }

    if let Some(values) = params.get("targets").and_then(|v| v.as_array()) {
        let targets: Vec<String> = values
            .iter()
            .filter_map(|value| value.as_str().map(String::from))
            .collect();
        if targets.is_empty() {
            return Err(RuvosError::InvalidParams(
                "missing non-empty 'targets' array".to_string(),
            ));
        }
        return Ok(targets);
    }

    if let Some(target) = params.get("to").and_then(|v| v.as_str()) {
        return Ok(vec![target.to_string()]);
    }

    Err(RuvosError::InvalidParams(
        "missing 'to' field or broadcast=true".to_string(),
    ))
}

fn finalize_swarm(
    requested_swarm_id: Option<&str>,
    status: &str,
    event_kind: &str,
    learning_detail: &str,
    mut payload: Value,
) -> Result<swarm::SwarmState> {
    let mut state = load_active_swarm(requested_swarm_id)?;
    state.status = status.to_string();
    state.updated_at = chrono::Utc::now().to_rfc3339();
    let stored = swarm::store(state)?;

    if let Some(map) = payload.as_object_mut() {
        map.insert("swarm_id".to_string(), json!(stored.id));
    }

    publish_event(RuntimeEvent {
        kind: event_kind.to_string(),
        payload,
        agent_id: None,
        task_id: None,
    });

    if let Err(error) = swarm::record_swarm_learning(&stored, status, learning_detail) {
        tracing::debug!(
            "swarm learning record failed for {}: {:?}",
            stored.id,
            error
        );
    }

    Ok(stored)
}

fn swarm_health(state: &swarm::SwarmState) -> Value {
    let now = chrono::Utc::now();
    let live_members = live_member_indices(state, now);
    let stale_members: Vec<&swarm::SwarmMember> = state
        .members
        .iter()
        .filter(|member| {
            !live_members
                .iter()
                .any(|live_index| state.members[*live_index].agent_id == member.agent_id)
        })
        .collect();
    let assigned_tasks = state
        .members
        .iter()
        .map(|member| member.assigned_tasks.len() as u64)
        .sum::<u64>();
    let active_agents = state
        .members
        .iter()
        .filter(|member| member.state == "active" || member.state == "assigned")
        .count() as u64;
    let health_score = if state.members.is_empty() {
        0.0
    } else {
        live_members.len() as f64 / state.members.len() as f64
    };

    json!({
        "swarm_id": state.id,
        "objective": state.objective,
        "topology": state.topology,
        "status": state.status,
        "coordinator": state.coordinator,
        "max_agents": state.max_agents,
        "member_count": state.members.len(),
        "live_members": live_members.len(),
        "stale_members": stale_members.len(),
        "active_agents": active_agents,
        "assigned_tasks": assigned_tasks,
        "health_score": health_score,
        "live_member_ids": live_members.iter().map(|index| state.members[*index].agent_id.clone()).collect::<Vec<_>>(),
        "stale_member_ids": stale_members.iter().map(|member| member.agent_id.clone()).collect::<Vec<_>>(),
    })
}

fn live_member_indices(
    state: &swarm::SwarmState,
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<usize> {
    state
        .members
        .iter()
        .enumerate()
        .filter_map(|(index, member)| {
            if member.state == "left" {
                return None;
            }
            chrono::DateTime::parse_from_rfc3339(&member.last_heartbeat)
                .ok()
                .and_then(|ts| {
                    let age = now.signed_duration_since(ts.with_timezone(&chrono::Utc));
                    (age.num_seconds() <= relay::TTL_SECS).then_some(index)
                })
        })
        .collect()
}

fn parse_members(params: &Value) -> Vec<swarm::SwarmMember> {
    params
        .get("members")
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    Some(swarm::SwarmMember {
                        agent_id: value.get("agent_id")?.as_str()?.to_string(),
                        role: value
                            .get("role")
                            .and_then(|v| v.as_str())
                            .unwrap_or("worker")
                            .to_string(),
                        state: value
                            .get("state")
                            .and_then(|v| v.as_str())
                            .unwrap_or("idle")
                            .to_string(),
                        capabilities: value
                            .get("capabilities")
                            .and_then(|v| v.as_array())
                            .map(|caps| {
                                caps.iter()
                                    .filter_map(|cap| cap.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        assigned_tasks: value
                            .get("assigned_tasks")
                            .and_then(|v| v.as_array())
                            .map(|tasks| {
                                tasks
                                    .iter()
                                    .filter_map(|task| task.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        last_heartbeat: value
                            .get("last_heartbeat")
                            .and_then(|v| v.as_str())
                            .map(String::from)
                            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

// ============================================================================
// swarm.create
// ============================================================================

pub struct SwarmCreateHandler;

impl ToolHandler for SwarmCreateHandler {
    fn name(&self) -> &'static str {
        "create"
    }
    fn domain(&self) -> &'static str {
        "swarm"
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["objective"],
            "properties": {
                "objective": { "type": "string", "description": "The swarm's goal or mission" },
                "topology": { "type": "string", "enum": ["hierarchical", "mesh", "hybrid", "adaptive"], "description": "Swarm communication topology" },
                "swarm_id": { "type": "string", "description": "Optional custom swarm ID" },
                "max_agents": { "type": "integer" }
            },
            "additionalProperties": false
        })
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("objective").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'objective' field (string)".to_string(),
            ));
        }
        if let Some(topology) = params.get("topology").and_then(|v| v.as_str()) {
            swarm::validate_topology(topology)?;
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let swarm_id = params
                .get("swarm_id")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let objective = params["objective"].as_str().unwrap_or_default().to_string();
            let max_agents = params
                .get("max_agents")
                .and_then(|v| v.as_u64())
                .unwrap_or(SWARM_CREATE_DEFAULT_MAX_AGENTS as u64)
                as u32;
            let coordinator = params
                .get("coordinator")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| relay::instance_id().to_string());
            let mut members = parse_members(&params);
            let topology_decision = infer_topology(&params, members.len(), max_agents);
            let topology = topology_decision.topology.clone();
            let topology_reason = topology_decision.reason.clone();
            if !members.iter().any(|member| member.agent_id == coordinator) {
                members.insert(
                    0,
                    swarm::SwarmMember {
                        agent_id: coordinator.clone(),
                        role: "coordinator".to_string(),
                        state: "active".to_string(),
                        capabilities: vec!["orchestrate".to_string(), "route".to_string()],
                        assigned_tasks: Vec::new(),
                        last_heartbeat: chrono::Utc::now().to_rfc3339(),
                    },
                );
            }

            let now = chrono::Utc::now().to_rfc3339();
            let state = swarm::SwarmState {
                id: swarm_id.clone(),
                objective: objective.clone(),
                topology: topology.clone(),
                coordinator: coordinator.clone(),
                max_agents,
                status: "active".to_string(),
                members: members.clone(),
                created_at: now.clone(),
                updated_at: now,
            };
            let stored = swarm::store(state)?;

            publish_event(RuntimeEvent {
                kind: "swarm.created".to_string(),
                payload: json!({
                    "swarm_id": stored.id,
                    "objective": stored.objective,
                    "topology": stored.topology,
                    "topology_reason": topology_reason,
                    "coordinator": stored.coordinator,
                    "max_agents": stored.max_agents,
                    "member_count": stored.members.len(),
                }),
                agent_id: None,
                task_id: None,
            });

            Ok(json!({
            "status": "created",
                "topology_source": topology_decision.source,
                "topology_reason": topology_reason,
                "swarm": stored
            }))
        })
    }
}

// ============================================================================
// swarm.status
// ============================================================================

pub struct SwarmStatusHandler;

impl ToolHandler for SwarmStatusHandler {
    fn name(&self) -> &'static str {
        "status"
    }
    fn domain(&self) -> &'static str {
        "swarm"
    }
    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let swarm_id = params.get("swarm_id").and_then(|v| v.as_str());
            let current = swarm::current();
            let found = match (swarm_id, current.as_ref()) {
                (Some(requested), Some(state)) => state.id == requested,
                (Some(_), None) => false,
                (None, Some(_)) => true,
                (None, None) => false,
            };

            publish_event(RuntimeEvent {
                kind: "swarm.status".to_string(),
                payload: json!({
                    "swarm_id": swarm_id,
                    "found": found,
                    "member_count": current.as_ref().map(|state| state.members.len()).unwrap_or(0),
                }),
                agent_id: None,
                task_id: None,
            });

            Ok(json!({
                "found": found,
                "swarm_id": swarm_id,
                "swarm": if found { current } else { None },
            }))
        })
    }
}

// ============================================================================
// swarm.assign
// ============================================================================

pub struct SwarmAssignHandler;

impl ToolHandler for SwarmAssignHandler {
    fn name(&self) -> &'static str {
        "assign"
    }
    fn domain(&self) -> &'static str {
        "swarm"
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["agent_id", "task_id"],
            "properties": {
                "agent_id": { "type": "string" },
                "task_id": { "type": "string" },
                "swarm_id": { "type": "string" },
                "description": { "type": "string" }
            },
            "additionalProperties": false
        })
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("agent_id").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'agent_id' field (string)".to_string(),
            ));
        }
        if params.get("task_id").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'task_id' field (string)".to_string(),
            ));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let requested_swarm_id = params.get("swarm_id").and_then(|v| v.as_str());
            let agent_id = params["agent_id"].as_str().unwrap_or_default().to_string();
            let task_id = params["task_id"].as_str().unwrap_or_default().to_string();
            let mut state = load_active_swarm(requested_swarm_id)?;

            let member = state
                .members
                .iter_mut()
                .find(|member| member.agent_id == agent_id)
                .ok_or_else(|| {
                    RuvosError::HandlerError(format!("unknown swarm member '{agent_id}'"))
                })?;

            if !member
                .assigned_tasks
                .iter()
                .any(|assigned| assigned == &task_id)
            {
                member.assigned_tasks.push(task_id.clone());
            }
            member.state = "assigned".to_string();
            state.updated_at = chrono::Utc::now().to_rfc3339();
            let stored = swarm::store(state)?;

            publish_event(RuntimeEvent {
                kind: "swarm.assigned".to_string(),
                payload: json!({
                    "swarm_id": stored.id,
                    "agent_id": agent_id,
                    "task_id": task_id,
                    "member_count": stored.members.len(),
                }),
                agent_id: None,
                task_id: None,
            });

            Ok(json!({
                "status": "assigned",
                "swarm": stored
            }))
        })
    }
}

// ============================================================================
// swarm.heartbeat
// ============================================================================

pub struct SwarmHeartbeatHandler;

impl ToolHandler for SwarmHeartbeatHandler {
    fn name(&self) -> &'static str {
        "heartbeat"
    }
    fn domain(&self) -> &'static str {
        "swarm"
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["agent_id"],
            "properties": {
                "agent_id": { "type": "string" },
                "swarm_id": { "type": "string" },
                "status": { "type": "string" }
            },
            "additionalProperties": false
        })
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("agent_id").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'agent_id' field (string)".to_string(),
            ));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let requested_swarm_id = params.get("swarm_id").and_then(|v| v.as_str());
            let agent_id = params["agent_id"].as_str().unwrap_or_default().to_string();
            let mut state = load_active_swarm(requested_swarm_id)?;
            let now = chrono::Utc::now().to_rfc3339();

            let member = state
                .members
                .iter_mut()
                .find(|member| member.agent_id == agent_id)
                .ok_or_else(|| {
                    RuvosError::HandlerError(format!("unknown swarm member '{agent_id}'"))
                })?;

            member.last_heartbeat = now.clone();
            if member.state == "idle" {
                member.state = "active".to_string();
            }
            state.updated_at = now.clone();
            let stored = swarm::store(state)?;

            publish_event(RuntimeEvent {
                kind: "swarm.heartbeat".to_string(),
                payload: json!({
                    "swarm_id": stored.id,
                    "agent_id": agent_id,
                    "last_heartbeat": now,
                    "member_count": stored.members.len(),
                }),
                agent_id: None,
                task_id: None,
            });

            Ok(json!({
                "status": "alive",
                "swarm": stored
            }))
        })
    }
}

// ============================================================================
// swarm.message
// ============================================================================

pub struct SwarmMessageHandler;

impl ToolHandler for SwarmMessageHandler {
    fn name(&self) -> &'static str {
        "message"
    }
    fn domain(&self) -> &'static str {
        "swarm"
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["body"],
            "properties": {
                "body": { "type": "string", "description": "Message content" },
                "to": { "type": "string", "description": "Target agent_id (or omit with broadcast=true)" },
                "broadcast": { "type": "boolean", "description": "Send to all swarm members", "default": false },
                "targets": { "type": "array", "items": { "type": "string" }, "description": "List of target agent IDs" },
                "swarm_id": { "type": "string" }
            },
            "additionalProperties": false
        })
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("body").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'body' field (string)".to_string(),
            ));
        }
        let has_to = params.get("to").and_then(|v| v.as_str()).is_some();
        let broadcast = params
            .get("broadcast")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let has_targets = params
            .get("targets")
            .and_then(|v| v.as_array())
            .map(|values| !values.is_empty())
            .unwrap_or(false);
        if !has_to && !broadcast && !has_targets {
            return Err(RuvosError::InvalidParams(
                "missing 'to', non-empty 'targets', or broadcast=true".to_string(),
            ));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let requested_swarm_id = params.get("swarm_id").and_then(|v| v.as_str());
            let body = params["body"].as_str().unwrap_or_default().to_string();
            let sender = params
                .get("from")
                .and_then(|v| v.as_str())
                .map(String::from)
                .unwrap_or_else(|| {
                    load_active_swarm(requested_swarm_id)
                        .map(|s| s.coordinator)
                        .unwrap_or_default()
                });
            let state = load_active_swarm(requested_swarm_id)?;
            let targets = message_targets(&params, &state, &sender)?;

            let mut delivered = Vec::new();
            let mut undelivered = Vec::new();
            for target in targets {
                if !member_exists(&state, &target) {
                    undelivered.push(json!({
                        "agent_id": target,
                        "reason": "unknown swarm member"
                    }));
                    continue;
                }

                match agent_store::append_message(&target, &body)? {
                    Some((message_id, message_count)) => {
                        delivered.push(json!({
                            "agent_id": target,
                            "message_id": message_id,
                            "message_count": message_count,
                        }));
                    }
                    None => {
                        undelivered.push(json!({
                            "agent_id": target,
                            "reason": "agent record not found"
                        }));
                    }
                }
            }

            publish_event(RuntimeEvent {
                kind: "swarm.message".to_string(),
                payload: json!({
                    "swarm_id": state.id,
                    "from": sender,
                    "body": body,
                    "delivered_count": delivered.len(),
                    "undelivered_count": undelivered.len(),
                }),
                agent_id: None,
                task_id: None,
            });

            Ok(json!({
                "status": if undelivered.is_empty() { "delivered" } else { "partial" },
                "swarm_id": state.id,
                "delivered": delivered,
                "undelivered": undelivered,
            }))
        })
    }
}

// ============================================================================
// swarm.complete
// ============================================================================

pub struct SwarmCompleteHandler;

impl ToolHandler for SwarmCompleteHandler {
    fn name(&self) -> &'static str {
        "complete"
    }
    fn domain(&self) -> &'static str {
        "swarm"
    }
    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let requested_swarm_id = params.get("swarm_id").and_then(|v| v.as_str());
            let summary = params
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let completed_by = params
                .get("completed_by")
                .and_then(|v| v.as_str())
                .map(String::from);

            let stored = finalize_swarm(
                requested_swarm_id,
                "completed",
                "swarm.completed",
                &summary,
                json!({
                    "swarm_id": requested_swarm_id,
                    "summary": summary,
                    "completed_by": completed_by,
                }),
            )?;

            Ok(json!({
                "status": "completed",
                "swarm": stored
            }))
        })
    }
}

// ============================================================================
// swarm.fail
// ============================================================================

pub struct SwarmFailHandler;

impl ToolHandler for SwarmFailHandler {
    fn name(&self) -> &'static str {
        "fail"
    }
    fn domain(&self) -> &'static str {
        "swarm"
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["reason"],
            "properties": {
                "reason": { "type": "string", "description": "Why the swarm failed" },
                "swarm_id": { "type": "string" },
                "failed_by": { "type": "string" }
            },
            "additionalProperties": false
        })
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("reason").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'reason' field (string)".to_string(),
            ));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let requested_swarm_id = params.get("swarm_id").and_then(|v| v.as_str());
            let reason = params["reason"].as_str().unwrap_or_default().to_string();
            let failed_by = params
                .get("failed_by")
                .and_then(|v| v.as_str())
                .map(String::from);

            let stored = finalize_swarm(
                requested_swarm_id,
                "failed",
                "swarm.failed",
                &reason,
                json!({
                    "swarm_id": requested_swarm_id,
                    "reason": reason,
                    "failed_by": failed_by,
                }),
            )?;

            Ok(json!({
                "status": "failed",
                "swarm": stored
            }))
        })
    }
}

// ============================================================================
// swarm.health
// ============================================================================

pub struct SwarmHealthHandler;

impl ToolHandler for SwarmHealthHandler {
    fn name(&self) -> &'static str {
        "health"
    }
    fn domain(&self) -> &'static str {
        "swarm"
    }
    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let requested_swarm_id = params.get("swarm_id").and_then(|v| v.as_str());
            let state = load_active_swarm(requested_swarm_id)?;
            let health = swarm_health(&state);

            publish_event(RuntimeEvent {
                kind: "swarm.health".to_string(),
                payload: health.clone(),
                agent_id: None,
                task_id: None,
            });

            Ok(json!({
                "status": "ok",
                "swarm": health,
            }))
        })
    }
}

// ============================================================================
// swarm.report
// ============================================================================

pub struct SwarmReportHandler;

impl ToolHandler for SwarmReportHandler {
    fn name(&self) -> &'static str {
        "report"
    }
    fn domain(&self) -> &'static str {
        "swarm"
    }
    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let requested_swarm_id = params.get("swarm_id").and_then(|v| v.as_str());
            let since = params.get("since").and_then(|v| v.as_i64()).unwrap_or(0);
            let state = load_active_swarm(requested_swarm_id)?;
            let (event_count, metrics, recent_events) = {
                let Some(store) = crate::store::try_store() else {
                    return Ok(json!({ "store_busy": true, "report": {} }));
                };
                let events = store
                    .events_since(since)
                    .map_err(|e| RuvosError::InternalError(format!("swarm report events: {e}")))?;
                let swarm_events = swarm_filter_events(&events, &state.id);
                let metrics = swarm_metrics(&state, &swarm_events);
                let recent_events: Vec<Value> = swarm_events
                    .iter()
                    .rev()
                    .take(25)
                    .map(|event| {
                        json!({
                            "id": event.id,
                            "event_type": event.event_type,
                            "agent_id": event.agent_id,
                            "task_id": event.task_id,
                            "payload": event.payload,
                            "timestamp": event.timestamp
                        })
                    })
                    .collect();
                (swarm_events.len(), metrics, recent_events)
            };

            publish_event(RuntimeEvent {
                kind: "swarm.report".to_string(),
                payload: json!({
                    "swarm_id": state.id,
                    "since": since,
                    "event_count": event_count,
                }),
                agent_id: None,
                task_id: None,
            });

            Ok(json!({
                "since": since,
                "report": {
                    "swarm": state,
                    "metrics": metrics,
                    "event_count": event_count,
                    "recent_events": recent_events,
                }
            }))
        })
    }
}

// ============================================================================
// swarm.metrics
// ============================================================================

pub struct SwarmMetricsHandler;

impl ToolHandler for SwarmMetricsHandler {
    fn name(&self) -> &'static str {
        "metrics"
    }
    fn domain(&self) -> &'static str {
        "swarm"
    }
    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let requested_swarm_id = params.get("swarm_id").and_then(|v| v.as_str());
            let since = params.get("since").and_then(|v| v.as_i64()).unwrap_or(0);
            let state = load_active_swarm(requested_swarm_id)?;
            let metrics = {
                let Some(store) = crate::store::try_store() else {
                    return Ok(json!({ "store_busy": true, "metrics": {} }));
                };
                let events = store
                    .events_since(since)
                    .map_err(|e| RuvosError::InternalError(format!("swarm metrics events: {e}")))?;
                let swarm_events = swarm_filter_events(&events, &state.id);
                swarm_metrics(&state, &swarm_events)
            };

            publish_event(RuntimeEvent {
                kind: "swarm.metrics".to_string(),
                payload: json!({
                    "swarm_id": state.id,
                    "since": since,
                    "metric_keys": metrics.as_object().map(|obj| obj.len()).unwrap_or(0),
                }),
                agent_id: None,
                task_id: None,
            });

            Ok(json!({
                "since": since,
                "metrics": metrics
            }))
        })
    }
}

// ============================================================================
// swarm.rebalance
// ============================================================================

pub struct SwarmRebalanceHandler;

impl ToolHandler for SwarmRebalanceHandler {
    fn name(&self) -> &'static str {
        "rebalance"
    }
    fn domain(&self) -> &'static str {
        "swarm"
    }
    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let requested_swarm_id = params.get("swarm_id").and_then(|v| v.as_str());
            let mut state = load_active_swarm(requested_swarm_id)?;
            let now = chrono::Utc::now();
            let live_member_indices = live_member_indices(&state, now);
            if live_member_indices.is_empty() {
                return Err(RuvosError::HandlerError(
                    "cannot rebalance a swarm with no live members".to_string(),
                ));
            }
            let task_target_indices: Vec<usize> = live_member_indices
                .iter()
                .copied()
                .filter(|index| state.members[*index].agent_id != state.coordinator)
                .collect();
            let task_target_indices = if task_target_indices.is_empty() {
                live_member_indices.clone()
            } else {
                task_target_indices
            };

            let mut stale_member_ids = Vec::new();
            let mut reassigned_tasks = Vec::new();
            let mut tasks_to_reassign: Vec<(String, String)> = Vec::new();

            for index in 0..state.members.len() {
                let is_live = live_member_indices.contains(&index);
                if is_live {
                    if state.members[index].state == "idle" {
                        state.members[index].state = "active".to_string();
                    }
                    continue;
                }

                let member = &mut state.members[index];
                let from_agent_id = member.agent_id.clone();
                if member.assigned_tasks.is_empty() {
                    member.state = "stale".to_string();
                    stale_member_ids.push(from_agent_id);
                    continue;
                }

                stale_member_ids.push(from_agent_id.clone());
                member.state = "stale".to_string();
                let tasks = std::mem::take(&mut member.assigned_tasks);
                tasks_to_reassign.extend(
                    tasks
                        .into_iter()
                        .map(|task_id| (from_agent_id.clone(), task_id)),
                );
            }

            for (live_cursor, (from_agent_id, task_id)) in tasks_to_reassign.into_iter().enumerate()
            {
                let target_index = task_target_indices[live_cursor % task_target_indices.len()];
                let target = &mut state.members[target_index];
                if !target
                    .assigned_tasks
                    .iter()
                    .any(|assigned| assigned == &task_id)
                {
                    target.assigned_tasks.push(task_id.clone());
                }
                if target.state == "idle" {
                    target.state = "active".to_string();
                }
                reassigned_tasks.push(json!({
                    "task_id": task_id,
                    "from": from_agent_id,
                    "to": target.agent_id,
                }));
            }

            state.updated_at = chrono::Utc::now().to_rfc3339();
            let stored = swarm::store(state)?;

            publish_event(RuntimeEvent {
                kind: "swarm.rebalanced".to_string(),
                payload: json!({
                    "swarm_id": stored.id,
                    "stale_members": stale_member_ids,
                    "reassigned_count": reassigned_tasks.len(),
                    "member_count": stored.members.len(),
                }),
                agent_id: None,
                task_id: None,
            });

            Ok(json!({
                "status": "rebalanced",
                "swarm": stored,
                "reassigned": reassigned_tasks,
            }))
        })
    }
}

// ============================================================================
// swarm.join
// ============================================================================

pub struct SwarmJoinHandler;

impl ToolHandler for SwarmJoinHandler {
    fn name(&self) -> &'static str {
        "join"
    }
    fn domain(&self) -> &'static str {
        "swarm"
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["agent_id"],
            "properties": {
                "agent_id": { "type": "string" },
                "swarm_id": { "type": "string" },
                "role": { "type": "string", "default": "worker" },
                "capabilities": { "type": "array", "items": { "type": "string" } }
            },
            "additionalProperties": false
        })
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("agent_id").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'agent_id' field (string)".to_string(),
            ));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let requested_swarm_id = params.get("swarm_id").and_then(|v| v.as_str());
            let agent_id = params["agent_id"].as_str().unwrap_or_default().to_string();
            let role = params
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or("worker")
                .to_string();
            let capabilities: Vec<String> = params
                .get("capabilities")
                .and_then(|v| v.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let now = chrono::Utc::now().to_rfc3339();
            let mut state = load_active_swarm(requested_swarm_id)?;
            let mut existed = false;

            if let Some(member) = state
                .members
                .iter_mut()
                .find(|member| member.agent_id == agent_id)
            {
                existed = true;
                member.role = role.clone();
                if !capabilities.is_empty() {
                    member.capabilities = capabilities.clone();
                }
                member.state = "active".to_string();
                member.last_heartbeat = now.clone();
            } else {
                state.members.push(swarm::SwarmMember {
                    agent_id: agent_id.clone(),
                    role: role.clone(),
                    state: "active".to_string(),
                    capabilities: capabilities.clone(),
                    assigned_tasks: Vec::new(),
                    last_heartbeat: now.clone(),
                });
            }

            state.updated_at = now.clone();
            let stored = swarm::store(state)?;

            publish_event(RuntimeEvent {
                kind: "swarm.joined".to_string(),
                payload: json!({
                    "swarm_id": stored.id,
                    "agent_id": agent_id,
                    "role": role,
                    "capabilities": capabilities,
                    "existed": existed,
                    "member_count": stored.members.len(),
                }),
                agent_id: None,
                task_id: None,
            });

            Ok(json!({
                "status": "joined",
                "swarm": stored
            }))
        })
    }
}

// ============================================================================
// swarm.leave
// ============================================================================

pub struct SwarmLeaveHandler;

impl ToolHandler for SwarmLeaveHandler {
    fn name(&self) -> &'static str {
        "leave"
    }
    fn domain(&self) -> &'static str {
        "swarm"
    }
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["agent_id"],
            "properties": {
                "agent_id": { "type": "string" },
                "swarm_id": { "type": "string" },
                "force": { "type": "boolean", "default": false }
            },
            "additionalProperties": false
        })
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("agent_id").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'agent_id' field (string)".to_string(),
            ));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let requested_swarm_id = params.get("swarm_id").and_then(|v| v.as_str());
            let agent_id = params["agent_id"].as_str().unwrap_or_default().to_string();
            let force = params
                .get("force")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let mut state = load_active_swarm(requested_swarm_id)?;
            let now = chrono::Utc::now().to_rfc3339();

            let member = state
                .members
                .iter_mut()
                .find(|member| member.agent_id == agent_id)
                .ok_or_else(|| {
                    RuvosError::HandlerError(format!("unknown swarm member '{agent_id}'"))
                })?;

            if !member.assigned_tasks.is_empty() && !force {
                return Err(RuvosError::HandlerError(
                    "member has assigned tasks; use force=true or rebalance first".to_string(),
                ));
            }

            member.state = "left".to_string();
            member.last_heartbeat = now.clone();
            state.updated_at = now.clone();
            let stored = swarm::store(state)?;

            publish_event(RuntimeEvent {
                kind: "swarm.left".to_string(),
                payload: json!({
                    "swarm_id": stored.id,
                    "agent_id": agent_id,
                    "forced": force,
                    "member_count": stored.members.len(),
                }),
                agent_id: None,
                task_id: None,
            });

            Ok(json!({
                "status": "left",
                "swarm": stored
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::gov::GovEventsHandler;

    fn isolate() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(dir.path().to_path_buf());
        dir
    }

    #[tokio::test]
    async fn create_and_status_roundtrip() {
        let _g = isolate();
        let created = SwarmCreateHandler
            .execute(json!({
                "objective": "ship a feature",
                "topology": "hierarchical",
                "max_agents": 4,
                "members": [
                    {"agent_id": "worker-1", "role": "coder", "capabilities": ["rust"]},
                    {"agent_id": "worker-2", "role": "tester", "capabilities": ["tests"]}
                ]
            }))
            .await
            .unwrap();
        assert_eq!(created["status"], "created");
        assert_eq!(created["swarm"]["topology"], "hierarchical");
        assert!(created["swarm"]["members"].as_array().unwrap().len() >= 2);

        let status = SwarmStatusHandler.execute(json!({})).await.unwrap();
        assert_eq!(status["found"], true);
        assert_eq!(status["swarm"]["objective"], "ship a feature");

        let events = GovEventsHandler
            .execute(json!({"event_type": "swarm.created", "limit": 10}))
            .await
            .unwrap();
        assert!(events["count"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn assign_and_heartbeat_update_member_state() {
        let _g = isolate();
        let created = SwarmCreateHandler
            .execute(json!({
                "objective": "ship a feature",
                "topology": "hierarchical",
                "members": [
                    {"agent_id": "worker-1", "role": "coder", "capabilities": ["rust"]}
                ]
            }))
            .await
            .unwrap();
        let swarm_id = created["swarm"]["id"].as_str().unwrap().to_string();

        let assigned = SwarmAssignHandler
            .execute(json!({
                "swarm_id": swarm_id,
                "agent_id": "worker-1",
                "task_id": "task-1"
            }))
            .await
            .unwrap();
        assert_eq!(assigned["status"], "assigned");
        assert_eq!(
            assigned["swarm"]["members"][1]["assigned_tasks"][0],
            "task-1"
        );

        let alive = SwarmHeartbeatHandler
            .execute(json!({
                "swarm_id": assigned["swarm"]["id"].as_str().unwrap(),
                "agent_id": "worker-1"
            }))
            .await
            .unwrap();
        assert_eq!(alive["status"], "alive");
        assert_eq!(alive["swarm"]["members"][1]["agent_id"], "worker-1");

        let assigned_events = GovEventsHandler
            .execute(json!({"event_type": "swarm.assigned", "limit": 10}))
            .await
            .unwrap();
        assert!(assigned_events["count"].as_u64().unwrap() >= 1);

        let heartbeat_events = GovEventsHandler
            .execute(json!({"event_type": "swarm.heartbeat", "limit": 10}))
            .await
            .unwrap();
        assert!(heartbeat_events["count"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn message_delivers_to_member_inbox() {
        let _g = isolate();
        let spawned = super::super::agent::AgentSpawnHandler
            .execute(json!({
                "archetype": "coder",
                "prompt": "seed agent",
                "model": "claude-haiku-4-5"
            }))
            .await
            .unwrap();
        let worker_id = spawned["agent_id"].as_str().unwrap().to_string();
        let created = SwarmCreateHandler
            .execute(json!({
                "objective": "coordinate",
                "topology": "hierarchical",
                "members": [
                    {"agent_id": worker_id.clone(), "role": "coder"}
                ]
            }))
            .await
            .unwrap();
        let swarm_id = created["swarm"]["id"].as_str().unwrap().to_string();

        let sent = SwarmMessageHandler
            .execute(json!({
                "swarm_id": swarm_id,
                "to": worker_id.clone(),
                "body": "finish task"
            }))
            .await
            .unwrap();
        assert_eq!(sent["status"], "delivered");
        assert_eq!(sent["delivered"].as_array().unwrap().len(), 1);
        assert_eq!(sent["undelivered"].as_array().unwrap().len(), 0);

        let status = super::super::agent::AgentStatusHandler
            .execute(json!({"agent_id": worker_id}))
            .await
            .unwrap();
        assert!(status["message_count"].as_u64().unwrap() >= 1);

        let events = GovEventsHandler
            .execute(json!({"event_type": "swarm.message", "limit": 10}))
            .await
            .unwrap();
        assert!(events["count"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn complete_and_fail_finalize_swarm_status() {
        let _g = isolate();
        let created = SwarmCreateHandler
            .execute(json!({
                "objective": "wrap up",
                "topology": "hierarchical",
                "members": [
                    {"agent_id": "worker-1", "role": "coder"}
                ]
            }))
            .await
            .unwrap();
        let swarm_id = created["swarm"]["id"].as_str().unwrap().to_string();

        let completed = SwarmCompleteHandler
            .execute(json!({
                "swarm_id": swarm_id,
                "summary": "finished the work",
                "completed_by": "worker-1"
            }))
            .await
            .unwrap();
        assert_eq!(completed["status"], "completed");
        assert_eq!(completed["swarm"]["status"], "completed");

        let completed_events = GovEventsHandler
            .execute(json!({"event_type": "swarm.completed", "limit": 10}))
            .await
            .unwrap();
        assert!(completed_events["count"].as_u64().unwrap() >= 1);

        let failed = SwarmFailHandler
            .execute(json!({
                "swarm_id": completed["swarm"]["id"].as_str().unwrap(),
                "reason": "post-check failed",
                "failed_by": "worker-1"
            }))
            .await
            .unwrap();
        assert_eq!(failed["status"], "failed");
        assert_eq!(failed["swarm"]["status"], "failed");

        let failed_events = GovEventsHandler
            .execute(json!({"event_type": "swarm.failed", "limit": 10}))
            .await
            .unwrap();
        assert!(failed_events["count"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn health_reports_freshness_and_utilization() {
        let _g = isolate();
        let created = SwarmCreateHandler
            .execute(json!({
                "objective": "monitor",
                "topology": "hierarchical",
                "members": [
                    {"agent_id": "worker-1", "role": "coder"},
                    {"agent_id": "worker-2", "role": "tester", "last_heartbeat": "2000-01-01T00:00:00Z"}
                ]
            }))
            .await
            .unwrap();
        let swarm_id = created["swarm"]["id"].as_str().unwrap().to_string();

        let health = SwarmHealthHandler
            .execute(json!({"swarm_id": swarm_id}))
            .await
            .unwrap();
        assert_eq!(health["status"], "ok");
        assert_eq!(health["swarm"]["member_count"], 3);
        assert!(health["swarm"]["live_members"].as_u64().unwrap() >= 1);
        assert!(health["swarm"]["stale_members"].as_u64().unwrap() >= 1);
        assert!(health["swarm"]["health_score"].as_f64().unwrap() >= 0.0);

        let events = GovEventsHandler
            .execute(json!({"event_type": "swarm.health", "limit": 10}))
            .await
            .unwrap();
        assert!(events["count"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn create_infers_topology_from_task_description() {
        let _g = isolate();
        let created = SwarmCreateHandler
            .execute(json!({
                "objective": "broadcast updates across peer workers",
                "members": [
                    {"agent_id": "worker-1", "role": "coder"},
                    {"agent_id": "worker-2", "role": "tester"}
                ]
            }))
            .await
            .unwrap();

        assert_eq!(created["status"], "created");
        assert_eq!(created["topology_source"], "inferred");
        assert_eq!(created["swarm"]["topology"], "mesh");
    }

    #[tokio::test]
    async fn report_and_metrics_summarize_swarm_state() {
        let _g = isolate();
        let created = SwarmCreateHandler
            .execute(json!({
                "objective": "reporting",
                "topology": "hierarchical",
                "members": [
                    {"agent_id": "worker-1", "role": "coder"},
                    {"agent_id": "worker-2", "role": "tester"}
                ]
            }))
            .await
            .unwrap();
        let swarm_id = created["swarm"]["id"].as_str().unwrap().to_string();
        SwarmAssignHandler
            .execute(json!({
                "swarm_id": swarm_id,
                "agent_id": "worker-1",
                "task_id": "task-1"
            }))
            .await
            .unwrap();

        let report = SwarmReportHandler
            .execute(json!({"swarm_id": created["swarm"]["id"].as_str().unwrap(), "since": 0}))
            .await
            .unwrap();
        assert_eq!(report["report"]["swarm"]["id"], created["swarm"]["id"]);
        assert!(
            report["report"]["metrics"]["assigned_tasks"]
                .as_u64()
                .unwrap()
                >= 1
        );
        assert!(report["report"]["event_count"].as_u64().unwrap() >= 1);

        let metrics = SwarmMetricsHandler
            .execute(json!({"swarm_id": created["swarm"]["id"].as_str().unwrap(), "since": 0}))
            .await
            .unwrap();
        assert!(metrics["metrics"]["health_score"].as_f64().unwrap() >= 0.0);
        assert!(metrics["metrics"]["event_counts"]
            .as_object()
            .unwrap()
            .contains_key("swarm.assigned"));

        let report_events = GovEventsHandler
            .execute(json!({"event_type": "swarm.report", "limit": 10}))
            .await
            .unwrap();
        assert!(report_events["count"].as_u64().unwrap() >= 1);
        let metrics_events = GovEventsHandler
            .execute(json!({"event_type": "swarm.metrics", "limit": 10}))
            .await
            .unwrap();
        assert!(metrics_events["count"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn rebalance_moves_tasks_off_stale_members() {
        let _g = isolate();
        let created = SwarmCreateHandler
            .execute(json!({
                "objective": "rebalance",
                "topology": "hierarchical",
                "members": [
                    {"agent_id": "worker-1", "role": "coder", "assigned_tasks": ["task-1"], "last_heartbeat": "2000-01-01T00:00:00Z"},
                    {"agent_id": "worker-2", "role": "tester"},
                    {"agent_id": "worker-3", "role": "reviewer"}
                ]
            }))
            .await
            .unwrap();
        let swarm_id = created["swarm"]["id"].as_str().unwrap().to_string();

        let rebalance = SwarmRebalanceHandler
            .execute(json!({"swarm_id": swarm_id}))
            .await
            .unwrap();
        assert_eq!(rebalance["status"], "rebalanced");
        assert!(!rebalance["reassigned"].as_array().unwrap().is_empty());
        assert_eq!(rebalance["swarm"]["status"], "active");
        let members = rebalance["swarm"]["members"].as_array().unwrap();
        assert!(
            members.iter().any(|member| {
                member["agent_id"] == "worker-2"
                    && !member["assigned_tasks"].as_array().unwrap().is_empty()
            }) || members.iter().any(|member| {
                member["agent_id"] == "worker-3"
                    && !member["assigned_tasks"].as_array().unwrap().is_empty()
            })
        );

        let events = GovEventsHandler
            .execute(json!({"event_type": "swarm.rebalanced", "limit": 10}))
            .await
            .unwrap();
        assert!(events["count"].as_u64().unwrap() >= 1);
    }

    #[tokio::test]
    async fn join_and_leave_update_membership_state() {
        let _g = isolate();
        let created = SwarmCreateHandler
            .execute(json!({
                "objective": "membership",
                "topology": "hierarchical"
            }))
            .await
            .unwrap();
        let swarm_id = created["swarm"]["id"].as_str().unwrap().to_string();

        let joined = SwarmJoinHandler
            .execute(json!({
                "swarm_id": swarm_id,
                "agent_id": "worker-1",
                "role": "coder",
                "capabilities": ["rust"]
            }))
            .await
            .unwrap();
        assert_eq!(joined["status"], "joined");
        assert!(joined["swarm"]["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|member| member["agent_id"] == "worker-1"));

        let left = SwarmLeaveHandler
            .execute(json!({
                "swarm_id": joined["swarm"]["id"].as_str().unwrap(),
                "agent_id": "worker-1"
            }))
            .await
            .unwrap();
        assert_eq!(left["status"], "left");
        assert!(left["swarm"]["members"]
            .as_array()
            .unwrap()
            .iter()
            .any(|member| member["agent_id"] == "worker-1" && member["state"] == "left"));

        let events = GovEventsHandler
            .execute(json!({"event_type": "swarm.left", "limit": 10}))
            .await
            .unwrap();
        assert!(events["count"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn validation_accepts_known_topologies() {
        assert!(SwarmCreateHandler
            .validate(&json!({"objective": "x", "topology": "mesh"}))
            .is_ok());
        assert!(SwarmCreateHandler
            .validate(&json!({"objective": "x", "topology": "bogus"}))
            .is_err());
    }
}
