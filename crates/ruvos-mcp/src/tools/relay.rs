//! Relay domain tools (3): announce, list, send.
//!
//! Cross-instance discovery + messaging between independently-launched Claude
//! Code instances, via pure file presence + mailboxes under
//! `$RUVOS_HOME/relays/`. See ADR-002 and [`crate::relay`].
//!
//! Every `announce`/`send` is recorded best-effort in the signed `gov.events`
//! audit log so cross-instance activity is tamper-evident.

use super::handler::{ExecuteFuture, ToolHandler};
use crate::relay;
use crate::{Result, RuvosError};
use ruvos_store::EventRecord;
use serde_json::{json, Value};

/// Best-effort `gov.events` write. Never fails the relay call — provenance is
/// auxiliary to the operation itself.
fn record_event(event_type: &str, payload: Value) {
    let ev = EventRecord::new(event_type, payload);
    let _ = crate::store::store().put_event(&ev);
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
            record_event(
                "relay.announce",
                json!({ "id": presence.id, "summary": presence.summary }),
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
                    record_event("relay.send", json!({ "to": to, "message_id": message_id }));
                    Ok(json!({ "delivered": true, "message_id": message_id }))
                }
                // Unknown / stale recipient is a normal outcome, not a tool error.
                Err(e) => Ok(json!({ "delivered": false, "error": e.message() })),
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
    }
}
