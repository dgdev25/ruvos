//! Relay engine: cross-instance presence + file mailboxes.
//!
//! A *relay node* is a live rUvOS MCP server process that announces its
//! presence so independently-launched Claude Code instances can discover and
//! message each other. Everything is pure files under
//! [`crate::paths::relays_dir`] — no daemon, no port, no SQLite, no polling.
//! See ADR-002.
//!
//! Layout under `relays_dir()`:
//! - `<id>.json`              — a [`Presence`] heartbeat record
//! - `<id>.inbox/<msgid>.json` — one [`RelayMessage`] per inbound message
//!
//! Liveness is derived from `updated_at` against [`TTL_SECS`]; stale relays are
//! pruned lazily when [`list`] runs.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::paths;
use crate::{Result, RuvosError};

/// A relay is considered live if `now - updated_at <= TTL_SECS`.
pub const TTL_SECS: i64 = 60;

/// Presence record for a live relay node (one per process).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Presence {
    pub id: String,
    pub pid: u32,
    pub cwd: String,
    pub git_repo: Option<String>,
    pub summary: String,
    pub updated_at: String,
}

/// A message dropped into a recipient's file mailbox.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RelayMessage {
    pub id: String,
    pub from: String,
    pub to: String,
    pub body: String,
    pub sent_at: String,
}

/// A durable collaboration contract between agents and instances.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoordinationRole {
    pub agent_id: String,
    pub role: String,
    pub responsibility: String,
}

/// A durable ownership / handoff record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoordinationContract {
    pub id: String,
    pub topic: String,
    pub owner: String,
    pub participants: Vec<String>,
    pub roles: Vec<CoordinationRole>,
    pub handoff_to: Option<String>,
    pub blockers: Vec<String>,
    pub status: String,
    pub resolution: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Stable instance id for this process: a v4 uuid generated once.
pub fn instance_id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| uuid::Uuid::new_v4().to_string())
}

fn presence_path(id: &str) -> PathBuf {
    paths::relays_dir().join(format!("{id}.json"))
}

fn inbox_dir(id: &str) -> PathBuf {
    paths::relays_dir().join(format!("{id}.inbox"))
}

fn contracts_path() -> PathBuf {
    paths::coordination_file()
}

/// Best-effort `git remote get-url origin` for the current working directory.
fn discover_git_repo() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if url.is_empty() {
        None
    } else {
        Some(url)
    }
}

/// Write `bytes` to `path` atomically (tmp file + rename).
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| RuvosError::InternalError(format!("relay mkdir: {e}")))?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)
        .map_err(|e| RuvosError::InternalError(format!("relay write: {e}")))?;
    std::fs::rename(&tmp, path)
        .map_err(|e| RuvosError::InternalError(format!("relay commit: {e}")))?;
    Ok(())
}

fn load_contracts() -> Vec<CoordinationContract> {
    let Ok(bytes) = std::fs::read(contracts_path()) else {
        return Vec::new();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn save_contracts(contracts: &[CoordinationContract]) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(contracts)
        .map_err(|e| RuvosError::InternalError(format!("coord serialize: {e}")))?;
    atomic_write(&contracts_path(), &bytes)
}

fn upsert_contract(contract: CoordinationContract) -> Result<CoordinationContract> {
    paths::ensure_root().map_err(|e| RuvosError::InternalError(format!("relay root: {e}")))?;
    std::fs::create_dir_all(paths::relays_dir())
        .map_err(|e| RuvosError::InternalError(format!("relay dir: {e}")))?;

    let mut contracts = load_contracts();
    if let Some(existing) = contracts
        .iter_mut()
        .find(|existing| existing.id == contract.id)
    {
        *existing = contract.clone();
    } else {
        contracts.push(contract.clone());
    }
    save_contracts(&contracts)?;
    Ok(contract)
}

fn get_contract(id: &str) -> Option<CoordinationContract> {
    load_contracts()
        .into_iter()
        .find(|contract| contract.id == id)
}

fn list_contracts(owner: Option<&str>, status: Option<&str>) -> Vec<CoordinationContract> {
    load_contracts()
        .into_iter()
        .filter(|contract| owner.map(|owner| contract.owner == owner).unwrap_or(true))
        .filter(|contract| {
            status
                .map(|status| contract.status == status)
                .unwrap_or(true)
        })
        .collect()
}

fn read_presence(path: &Path) -> Option<Presence> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice::<Presence>(&bytes).ok()
}

/// True if `updated_at` is within [`TTL_SECS`] of now.
fn is_live(updated_at: &str) -> bool {
    match chrono::DateTime::parse_from_rfc3339(updated_at) {
        Ok(ts) => {
            let age = chrono::Utc::now().signed_duration_since(ts.with_timezone(&chrono::Utc));
            age.num_seconds() <= TTL_SECS
        }
        Err(_) => false,
    }
}

/// Remove a relay's presence file and its inbox directory.
fn prune(id: &str) {
    let _ = std::fs::remove_file(presence_path(id));
    let _ = std::fs::remove_dir_all(inbox_dir(id));
}

/// Register / refresh this instance's presence, returning the written record.
pub fn announce(summary: &str) -> Result<Presence> {
    paths::ensure_root().map_err(|e| RuvosError::InternalError(format!("relay root: {e}")))?;
    std::fs::create_dir_all(paths::relays_dir())
        .map_err(|e| RuvosError::InternalError(format!("relay dir: {e}")))?;

    let cwd = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let presence = Presence {
        id: instance_id().to_string(),
        pid: std::process::id(),
        cwd,
        git_repo: discover_git_repo(),
        summary: summary.to_string(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };

    let bytes = serde_json::to_vec_pretty(&presence)
        .map_err(|e| RuvosError::InternalError(format!("relay serialize: {e}")))?;
    atomic_write(&presence_path(&presence.id), &bytes)?;
    Ok(presence)
}

/// Discover other live relays, applying a scope filter and pruning stale ones.
///
/// `scope`: `"machine"` = all; `"directory"` = same cwd as self;
/// `"repo"` = same git_repo as self. The calling instance is always excluded.
pub fn list(scope: &str) -> Result<Vec<Presence>> {
    let dir = paths::relays_dir();
    let me = instance_id();

    // Self presence is the reference for directory/repo scope filtering.
    let self_presence = read_presence(&presence_path(me));

    let rd = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(_) => return Ok(Vec::new()), // no relays dir yet → nobody around
    };

    let mut out = Vec::new();
    for entry in rd.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map(|x| x != "json").unwrap_or(true) {
            continue;
        }
        let presence = match read_presence(&path) {
            Some(p) => p,
            None => continue,
        };
        if !is_live(&presence.updated_at) {
            prune(&presence.id); // lazy staleness prune (presence + inbox)
            continue;
        }
        if presence.id == me {
            continue; // never list self
        }
        let keep = match scope {
            "directory" => self_presence
                .as_ref()
                .map(|s| s.cwd == presence.cwd)
                .unwrap_or(false),
            "repo" => self_presence
                .as_ref()
                .map(|s| s.git_repo.is_some() && s.git_repo == presence.git_repo)
                .unwrap_or(false),
            _ => true, // "machine" and any other value
        };
        if keep {
            out.push(presence);
        }
    }
    out.sort_by(|a, b| a.updated_at.cmp(&b.updated_at));
    Ok(out)
}

/// Deliver a message to `to`'s inbox. Errors if the recipient is unknown or
/// stale. Returns the new message id.
pub fn send(to: &str, body: &str) -> Result<String> {
    let presence = read_presence(&presence_path(to));
    match presence {
        Some(p) if is_live(&p.updated_at) => {}
        Some(_) => {
            return Err(RuvosError::InvalidParams(format!(
                "recipient '{to}' is stale (no recent presence)"
            )))
        }
        None => {
            return Err(RuvosError::InvalidParams(format!(
                "unknown recipient '{to}'"
            )))
        }
    }

    let msg = RelayMessage {
        id: uuid::Uuid::new_v4().to_string(),
        from: instance_id().to_string(),
        to: to.to_string(),
        body: body.to_string(),
        sent_at: chrono::Utc::now().to_rfc3339(),
    };
    let bytes = serde_json::to_vec_pretty(&msg)
        .map_err(|e| RuvosError::InternalError(format!("relay msg serialize: {e}")))?;
    atomic_write(&inbox_dir(to).join(format!("{}.json", msg.id)), &bytes)?;
    Ok(msg.id)
}

/// Read and delete all messages in `id`'s inbox, returning them sorted by
/// `sent_at`.
pub fn drain_inbox(id: &str) -> Result<Vec<RelayMessage>> {
    let dir = inbox_dir(id);
    let rd = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(_) => return Ok(Vec::new()),
    };

    let mut out = Vec::new();
    for entry in rd.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map(|x| x != "json").unwrap_or(true) {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(msg) = serde_json::from_slice::<RelayMessage>(&bytes) {
                out.push(msg);
            }
        }
        let _ = std::fs::remove_file(&path);
    }
    out.sort_by(|a, b| a.sent_at.cmp(&b.sent_at));
    Ok(out)
}

/// Store or update a durable coordination contract.
pub fn store_contract(mut contract: CoordinationContract) -> Result<CoordinationContract> {
    if contract.id.is_empty() {
        contract.id = uuid::Uuid::new_v4().to_string();
    }
    let now = chrono::Utc::now().to_rfc3339();
    if contract.created_at.is_empty() {
        contract.created_at = now.clone();
    }
    contract.updated_at = now;
    upsert_contract(contract)
}

/// Fetch a contract by id.
pub fn fetch_contract(id: &str) -> Option<CoordinationContract> {
    get_contract(id)
}

/// List contracts filtered by owner and/or status.
pub fn contracts(owner: Option<&str>, status: Option<&str>) -> Vec<CoordinationContract> {
    list_contracts(owner, status)
}

/// Resolve a contract into a final state.
pub fn resolve_contract(
    id: &str,
    resolution: &str,
    status: &str,
    handoff_to: Option<&str>,
) -> Result<Option<CoordinationContract>> {
    let mut contracts = load_contracts();
    let Some(contract) = contracts.iter_mut().find(|contract| contract.id == id) else {
        return Ok(None);
    };
    contract.resolution = Some(resolution.to_string());
    contract.status = status.to_string();
    contract.handoff_to = handoff_to
        .map(String::from)
        .or_else(|| contract.handoff_to.clone());
    contract.updated_at = chrono::Utc::now().to_rfc3339();
    let updated = contract.clone();
    save_contracts(&contracts)?;
    Ok(Some(updated))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolate() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(dir.path().to_path_buf());
        dir
    }

    /// Write a second instance's presence file directly (simulating another
    /// process), since `instance_id()` is a per-process `OnceLock`.
    fn write_presence(id: &str, cwd: &str, git_repo: Option<&str>, fresh: bool) {
        std::fs::create_dir_all(paths::relays_dir()).unwrap();
        let updated_at = if fresh {
            chrono::Utc::now().to_rfc3339()
        } else {
            (chrono::Utc::now() - chrono::Duration::seconds(TTL_SECS + 30)).to_rfc3339()
        };
        let p = Presence {
            id: id.to_string(),
            pid: 1234,
            cwd: cwd.to_string(),
            git_repo: git_repo.map(|s| s.to_string()),
            summary: "peer".into(),
            updated_at,
        };
        let bytes = serde_json::to_vec(&p).unwrap();
        std::fs::write(presence_path(id), bytes).unwrap();
    }

    #[test]
    fn announce_writes_presence_file() {
        let _g = isolate();
        let p = announce("working on backend").unwrap();
        assert_eq!(p.id, instance_id());
        assert!(p.pid > 0);
        assert_eq!(p.summary, "working on backend");
        assert!(presence_path(instance_id()).exists());
    }

    #[test]
    fn list_excludes_self_and_includes_live_peer() {
        let _g = isolate();
        announce("me").unwrap();
        write_presence("peer-a", "/somewhere", None, true);

        let peers = list("machine").unwrap();
        assert_eq!(peers.len(), 1, "should see exactly the one peer, not self");
        assert_eq!(peers[0].id, "peer-a");
    }

    #[test]
    fn list_prunes_stale_presence_and_inbox() {
        let _g = isolate();
        announce("me").unwrap();
        write_presence("ghost", "/x", None, false);
        // Give the ghost an inbox to confirm it gets pruned too.
        std::fs::create_dir_all(inbox_dir("ghost")).unwrap();
        std::fs::write(inbox_dir("ghost").join("m.json"), b"{}").unwrap();

        let peers = list("machine").unwrap();
        assert!(peers.is_empty(), "stale peer must not be listed");
        assert!(!presence_path("ghost").exists(), "stale presence pruned");
        assert!(!inbox_dir("ghost").exists(), "stale inbox pruned");
    }

    #[test]
    fn list_scope_repo_filters_by_remote() {
        let _g = isolate();
        // Self presence must carry a git_repo for repo scope to match.
        let mut me = announce("me").unwrap();
        me.git_repo = Some("git@example.com:org/repo.git".into());
        std::fs::write(presence_path(&me.id), serde_json::to_vec(&me).unwrap()).unwrap();

        write_presence(
            "same-repo",
            "/a",
            Some("git@example.com:org/repo.git"),
            true,
        );
        write_presence(
            "other-repo",
            "/b",
            Some("git@example.com:org/other.git"),
            true,
        );
        write_presence("no-repo", "/c", None, true);

        let peers = list("repo").unwrap();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].id, "same-repo");

        // machine scope sees all three peers.
        assert_eq!(list("machine").unwrap().len(), 3);
    }

    #[test]
    fn send_rejects_unknown_and_stale_recipient() {
        let _g = isolate();
        announce("me").unwrap();
        assert!(send("nobody", "hi").is_err(), "unknown recipient rejected");

        write_presence("ghost", "/x", None, false);
        assert!(send("ghost", "hi").is_err(), "stale recipient rejected");
    }

    #[test]
    fn send_then_drain_roundtrips() {
        let _g = isolate();
        announce("me").unwrap();
        write_presence("peer-b", "/y", None, true);

        let id1 = send("peer-b", "first").unwrap();
        let id2 = send("peer-b", "second").unwrap();
        assert_ne!(id1, id2);

        let msgs = drain_inbox("peer-b").unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].body, "first");
        assert_eq!(msgs[1].body, "second");
        assert_eq!(msgs[0].from, instance_id());

        // Drained: inbox is now empty.
        assert!(drain_inbox("peer-b").unwrap().is_empty());
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
        let stored = store_contract(contract).unwrap();
        let fetched = fetch_contract(&stored.id).unwrap();
        assert_eq!(fetched.topic, "release");
        assert_eq!(contracts(Some("agent-a"), Some("open")).len(), 1);

        let resolved = resolve_contract(&stored.id, "approved", "resolved", None)
            .unwrap()
            .unwrap();
        assert_eq!(resolved.status, "resolved");
        assert_eq!(resolved.resolution.as_deref(), Some("approved"));
    }
}
