//! Relay domain tools (3): announce, list, send.
//!
//! Cross-instance discovery + messaging between independently-launched Claude
//! Code instances, via pure file presence + mailboxes under
//! `$RUVOS_HOME/relays/`. See ADR-002 and [`crate::relay`].
//!
//! Every `announce`/`send` is recorded best-effort in the signed `gov.events`
//! audit log so cross-instance activity is tamper-evident.

use super::handler::{ExecuteFuture, ToolHandler};
use crate::relay::{self, CoordinationContract, CoordinationRole};
use crate::runtime::{publish_event, RuntimeEvent};
use crate::{Result, RuvosError};
use ruvos_store::EventRecord;
use serde_json::{json, Value};

/// Best-effort `gov.events` write. Never fails the relay call — provenance is
/// auxiliary to the operation itself.
fn record_event(event_type: &str, payload: Value) {
    if let Some(s) = crate::store::try_store() {
        let ev = EventRecord::new(event_type, payload);
        let _ = s.put_event(&ev);
    }
}

// ============================================================================
// relay.announce
// ============================================================================

pub struct RelayAnnounceHandler;

impl ToolHandler for RelayAnnounceHandler {
    fn name(&self) -> &'static str {
        "announce"
    }
    fn domain(&self) -> &'static str {
        "relay"
    }
    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let summary = params
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let presence = relay::announce(summary)?;
            let presence_id = presence.id.clone();
            let presence_summary = presence.summary.clone();
            publish_event(RuntimeEvent {
                kind: "relay.announce".to_string(),
                payload: json!({ "id": presence_id.clone(), "summary": presence_summary.clone() }),
                agent_id: None,
                task_id: None,
            });
            record_event(
                "relay.announce",
                json!({ "id": presence_id, "summary": presence_summary }),
            );
            serde_json::to_value(&presence)
                .map_err(|e| RuvosError::InternalError(format!("serialize presence: {e}")))
        })
    }
}

// ============================================================================
// relay.list
// ============================================================================

pub struct RelayListHandler;

impl ToolHandler for RelayListHandler {
    fn name(&self) -> &'static str {
        "list"
    }
    fn domain(&self) -> &'static str {
        "relay"
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if let Some(scope) = params.get("scope").and_then(|v| v.as_str()) {
            if !matches!(scope, "machine" | "directory" | "repo") {
                return Err(RuvosError::InvalidParams(format!(
                    "invalid scope '{scope}' (expected machine|directory|repo)"
                )));
            }
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let scope = params
                .get("scope")
                .and_then(|v| v.as_str())
                .unwrap_or("machine")
                .to_string();
            let relays = relay::list(&scope)?;
            let inbox = relay::drain_inbox(relay::instance_id())?;
            let scope_value = scope.clone();
            publish_event(RuntimeEvent {
                kind: "relay.list".to_string(),
                payload: json!({
                    "scope": scope_value,
                    "count": relays.len(),
                    "inbox_count": inbox.len(),
                }),
                agent_id: None,
                task_id: None,
            });
            Ok(json!({
                "scope": scope,
                "count": relays.len(),
                "relays": relays,
                "inbox": inbox,
            }))
        })
    }
}

// ============================================================================
// relay.send
// ============================================================================

pub struct RelaySendHandler;

impl ToolHandler for RelaySendHandler {
    fn name(&self) -> &'static str {
        "send"
    }
    fn domain(&self) -> &'static str {
        "relay"
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("to").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'to' field (string)".to_string(),
            ));
        }
        if params.get("body").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'body' field (string)".to_string(),
            ));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let to = params["to"].as_str().unwrap_or_default().to_string();
            let body = params["body"].as_str().unwrap_or_default().to_string();
            match relay::send(&to, &body) {
                Ok(message_id) => {
                    let event_message_id = message_id.clone();
                    publish_event(RuntimeEvent {
                        kind: "relay.send".to_string(),
                        payload: json!({ "to": to.clone(), "message_id": event_message_id }),
                        agent_id: None,
                        task_id: None,
                    });
                    record_event("relay.send", json!({ "to": to, "message_id": message_id }));
                    Ok(json!({ "delivered": true, "message_id": message_id }))
                }
                // Unknown / stale recipient is a normal outcome, not a tool error.
                Err(e) => Ok(json!({ "delivered": false, "error": e.message() })),
            }
        })
    }
}

// ============================================================================
// relay.contract_store
// ============================================================================

pub struct RelayContractStoreHandler;

impl ToolHandler for RelayContractStoreHandler {
    fn name(&self) -> &'static str {
        "contract_store"
    }
    fn domain(&self) -> &'static str {
        "relay"
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("topic").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'topic' field (string)".to_string(),
            ));
        }
        if params.get("owner").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'owner' field (string)".to_string(),
            ));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let topic = params["topic"].as_str().unwrap_or_default().to_string();
            let owner = params["owner"].as_str().unwrap_or_default().to_string();
            let participants: Vec<String> = params
                .get("participants")
                .and_then(|v| v.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let roles: Vec<CoordinationRole> = params
                .get("roles")
                .and_then(|v| v.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| {
                            Some(CoordinationRole {
                                agent_id: value.get("agent_id")?.as_str()?.to_string(),
                                role: value.get("role")?.as_str()?.to_string(),
                                responsibility: value
                                    .get("responsibility")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            let handoff_to = params
                .get("handoff_to")
                .and_then(|v| v.as_str())
                .map(String::from);
            let blockers: Vec<String> = params
                .get("blockers")
                .and_then(|v| v.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let status = params
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("open")
                .to_string();
            let contract = relay::store_contract(CoordinationContract {
                id: params
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                topic: topic.clone(),
                owner: owner.clone(),
                participants: participants.clone(),
                roles: roles.clone(),
                handoff_to: handoff_to.clone(),
                blockers: blockers.clone(),
                status: status.clone(),
                resolution: params
                    .get("resolution")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                created_at: params
                    .get("created_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
                updated_at: String::new(),
            })?;

            publish_event(RuntimeEvent {
                kind: "relay.contract.stored".to_string(),
                payload: json!({
                    "contract_id": contract.id,
                    "topic": topic,
                    "owner": owner,
                    "participants": participants,
                    "status": status,
                }),
                agent_id: None,
                task_id: None,
            });
            record_event(
                "relay.contract.stored",
                json!({
                    "contract_id": contract.id,
                    "topic": contract.topic,
                    "owner": contract.owner,
                    "participants": contract.participants,
                    "status": contract.status,
                }),
            );
            serde_json::to_value(&contract)
                .map_err(|e| RuvosError::InternalError(format!("serialize contract: {e}")))
        })
    }
}

// ============================================================================
// relay.contracts
// ============================================================================

pub struct RelayContractsHandler;

impl ToolHandler for RelayContractsHandler {
    fn name(&self) -> &'static str {
        "contracts"
    }
    fn domain(&self) -> &'static str {
        "relay"
    }
    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let owner = params.get("owner").and_then(|v| v.as_str());
            let status = params.get("status").and_then(|v| v.as_str());
            let contracts = relay::contracts(owner, status);
            publish_event(RuntimeEvent {
                kind: "relay.contracts.listed".to_string(),
                payload: json!({
                    "count": contracts.len(),
                    "owner": owner,
                    "status": status,
                }),
                agent_id: None,
                task_id: None,
            });
            Ok(json!({
                "count": contracts.len(),
                "contracts": contracts
            }))
        })
    }
}

// ============================================================================
// relay.contract_resolve
// ============================================================================

pub struct RelayContractResolveHandler;

impl ToolHandler for RelayContractResolveHandler {
    fn name(&self) -> &'static str {
        "contract_resolve"
    }
    fn domain(&self) -> &'static str {
        "relay"
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("id").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'id' field (string)".to_string(),
            ));
        }
        if params.get("resolution").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'resolution' field (string)".to_string(),
            ));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let id = params["id"].as_str().unwrap_or_default().to_string();
            let resolution = params["resolution"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let status = params
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("resolved")
                .to_string();
            let handoff_to = params.get("handoff_to").and_then(|v| v.as_str());

            match relay::resolve_contract(&id, &resolution, &status, handoff_to)? {
                Some(contract) => {
                    if let Some(target) = contract.handoff_to.as_deref() {
                        let _ = relay::send(target, &format!("handoff:{id}:{status}"));
                    }
                    publish_event(RuntimeEvent {
                        kind: "relay.contract.resolved".to_string(),
                        payload: json!({
                            "contract_id": id,
                            "status": contract.status,
                            "resolution": contract.resolution,
                            "handoff_to": contract.handoff_to,
                        }),
                        agent_id: None,
                        task_id: None,
                    });
                    record_event(
                        "relay.contract.resolved",
                        json!({
                            "contract_id": contract.id,
                            "status": contract.status,
                            "resolution": contract.resolution,
                            "handoff_to": contract.handoff_to,
                        }),
                    );
                    Ok(json!({
                        "found": true,
                        "contract": contract
                    }))
                }
                None => Ok(json!({
                    "found": false,
                    "contract_id": id
                })),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::{instance_id, Presence, RelayMessage};

    fn isolate() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(dir.path().to_path_buf());
        dir
    }

    /// Write a second instance's presence directly (separate "process").
    fn write_presence(id: &str) {
        let dir = crate::paths::relays_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let p = Presence {
            id: id.to_string(),
            pid: 4321,
            cwd: "/peer".into(),
            git_repo: None,
            summary: "peer".into(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        std::fs::write(
            dir.join(format!("{id}.json")),
            serde_json::to_vec(&p).unwrap(),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn announce_then_list_sees_peer_not_self() {
        let _g = isolate();
        RelayAnnounceHandler
            .execute(json!({"summary": "me"}))
            .await
            .unwrap();
        write_presence("peer-x");

        let r = RelayListHandler
            .execute(json!({"scope": "machine"}))
            .await
            .unwrap();
        assert_eq!(r["count"], 1);
        assert_eq!(r["relays"][0]["id"], "peer-x");
        // self id never appears
        assert!(r["relays"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["id"] != instance_id()));
    }

    #[tokio::test]
    async fn send_to_peer_then_drain_returns_message() {
        let _g = isolate();
        RelayAnnounceHandler.execute(json!({})).await.unwrap();
        write_presence("peer-y");

        let sent = RelaySendHandler
            .execute(json!({"to": "peer-y", "body": "ping"}))
            .await
            .unwrap();
        assert_eq!(sent["delivered"], true);

        let msgs = crate::relay::drain_inbox("peer-y").unwrap();
        assert_eq!(msgs.len(), 1);
        let m: &RelayMessage = &msgs[0];
        assert_eq!(m.body, "ping");
        assert_eq!(m.from, instance_id());
    }

    #[tokio::test]
    async fn send_to_missing_recipient_is_not_delivered() {
        let _g = isolate();
        RelayAnnounceHandler.execute(json!({})).await.unwrap();
        let r = RelaySendHandler
            .execute(json!({"to": "ghost", "body": "x"}))
            .await
            .unwrap();
        assert_eq!(r["delivered"], false);
        assert!(r["error"].as_str().unwrap().contains("ghost"));
    }

    #[test]
    fn validation() {
        assert!(RelayAnnounceHandler.validate(&json!({})).is_ok());
        assert!(RelayListHandler.validate(&json!({})).is_ok());
        assert!(RelayListHandler
            .validate(&json!({"scope": "bogus"}))
            .is_err());
        assert!(RelaySendHandler.validate(&json!({"to": "x"})).is_err());
        assert!(RelaySendHandler
            .validate(&json!({"to": "x", "body": "y"}))
            .is_ok());
        assert!(RelayContractStoreHandler
            .validate(&json!({"topic": "x", "owner": "y"}))
            .is_ok());
        assert!(RelayContractResolveHandler
            .validate(&json!({"id": "x", "resolution": "y"}))
            .is_ok());
    }

    #[test]
    fn contracts_roundtrip_and_resolve() {
        let _g = isolate();
        let contract = CoordinationContract {
            id: String::new(),
            topic: "release".into(),
            owner: "agent-a".into(),
            participants: vec!["agent-b".into()],
            roles: vec![CoordinationRole {
                agent_id: "agent-a".into(),
                role: "owner".into(),
                responsibility: "ship safely".into(),
            }],
            handoff_to: Some("agent-b".into()),
            blockers: vec!["review".into()],
            status: "open".into(),
            resolution: None,
            created_at: String::new(),
            updated_at: String::new(),
        };

        let stored = relay::store_contract(contract).unwrap();
        let fetched = relay::fetch_contract(&stored.id).unwrap();
        assert_eq!(fetched.topic, "release");
        assert_eq!(relay::contracts(Some("agent-a"), Some("open")).len(), 1);

        let resolved = relay::resolve_contract(&stored.id, "approved", "resolved", None)
            .unwrap()
            .unwrap();
        assert_eq!(resolved.status, "resolved");
        assert_eq!(resolved.resolution.as_deref(), Some("approved"));
    }
}
