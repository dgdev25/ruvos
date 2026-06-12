//! `ruvos status` — human-facing system-state view (read-only).
//!
//! Reuses the exact handlers behind the gov/swarm/agent/relay MCP tools so
//! the CLI and the MCP surface can never disagree about system state.

use anyhow::Result;
use ruvos_mcp::tools::handler::ToolHandler;
use serde_json::{json, Value};

/// Run one tool handler, mapping a handler error to a JSON error marker so a
/// broken store degrades one section instead of killing the whole view.
async fn section(handler: &dyn ToolHandler, params: Value) -> Value {
    match handler.execute(params).await {
        Ok(v) => v,
        Err(e) => json!({"error": e.message()}),
    }
}

/// Collect the merged system-state view from the gov/swarm/agent/relay
/// handlers. Each section degrades independently on error.
pub async fn collect_status() -> Result<Value> {
    use ruvos_mcp::tools::{agent, gov, relay};
    Ok(json!({
        "health": section(&gov::GovHealthHandler, json!({})).await,
        "swarm": section(&gov::GovSwarmStatusHandler, json!({})).await,
        "agents": section(&agent::AgentStatusHandler, json!({})).await,
        "events": section(&gov::GovEventsHandler, json!({"limit": 10})).await,
        "relays": section(&relay::RelayListHandler, json!({})).await,
    }))
}

/// Entry point for `ruvos status`: collect, then print human or JSON view.
pub async fn run(json: bool) -> Result<()> {
    let v = collect_status().await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        print!("{}", render_status(&v));
    }
    Ok(())
}

fn section_line(out: &mut String, title: &str) {
    out.push_str(&format!("\n── {title} ──────────────────────────────\n"));
}

/// Format a unix-seconds timestamp as RFC 3339 UTC (best-effort).
fn fmt_ts(v: &Value) -> String {
    match v
        .as_i64()
        .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
    {
        Some(dt) => dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        None => v.as_str().unwrap_or("").to_string(),
    }
}

fn render_health(out: &mut String, v: &Value) {
    section_line(out, "Health");
    if let Some(e) = v.get("error") {
        out.push_str(&format!("  unavailable: {e}\n"));
        return;
    }
    out.push_str(&format!(
        "  status: {}  version: {}  pid: {}  tools: {}\n  data root: {}\n",
        v["status"].as_str().unwrap_or("?"),
        v["version"].as_str().unwrap_or("?"),
        v["pid"],
        v["tool_count"],
        v["data_root"].as_str().unwrap_or("?"),
    ));
    if let Some(p) = v["persisted"].as_object() {
        out.push_str(&format!(
            "  persisted: {} session(s), {} memory entr(ies), {} agent(s), {} intel pattern(s)\n",
            p.get("sessions").cloned().unwrap_or_default(),
            p.get("memory_entries").cloned().unwrap_or_default(),
            p.get("agents").cloned().unwrap_or_default(),
            p.get("intel_patterns").cloned().unwrap_or_default(),
        ));
    }
}

fn render_swarm(out: &mut String, v: &Value) {
    section_line(out, "Swarm");
    if let Some(e) = v.get("error") {
        out.push_str(&format!("  unavailable: {e}\n"));
        return;
    }
    if v["exists"].as_bool() != Some(true) {
        out.push_str("  no active swarm\n");
        return;
    }
    let s = &v["state"];
    out.push_str(&format!(
        "  id: {}  topology: {}  status: {}\n  objective: {}\n",
        s["id"].as_str().unwrap_or("?"),
        s["topology"].as_str().unwrap_or("?"),
        s["status"].as_str().unwrap_or("?"),
        s["objective"].as_str().unwrap_or("?"),
    ));
    if let Some(members) = s["members"].as_array() {
        for m in members {
            out.push_str(&format!(
                "    {} [{}] {} — {} task(s)\n",
                m["agent_id"].as_str().unwrap_or("?"),
                m["role"].as_str().unwrap_or("?"),
                m["state"].as_str().unwrap_or("?"),
                m["assigned_tasks"].as_array().map(|a| a.len()).unwrap_or(0),
            ));
        }
    }
}

fn render_agents(out: &mut String, v: &Value) {
    section_line(out, "Agents");
    if let Some(e) = v.get("error") {
        out.push_str(&format!("  unavailable: {e}\n"));
        return;
    }
    match v["agents"].as_array() {
        Some(agents) if !agents.is_empty() => {
            for a in agents {
                out.push_str(&format!(
                    "  {} [{}] {} — {} message(s), created {}\n",
                    a["agent_id"].as_str().unwrap_or("?"),
                    a["archetype"].as_str().unwrap_or("?"),
                    a["status"].as_str().unwrap_or("?"),
                    a["message_count"],
                    a["created_at"].as_str().unwrap_or("?"),
                ));
            }
        }
        _ => out.push_str("  none\n"),
    }
}

fn render_events(out: &mut String, v: &Value) {
    section_line(out, "Recent events");
    if let Some(e) = v.get("error") {
        out.push_str(&format!("  unavailable: {e}\n"));
        return;
    }
    match v["events"].as_array() {
        Some(events) if !events.is_empty() => {
            for e in events {
                let agent = e["agent_id"].as_str().unwrap_or("-");
                out.push_str(&format!(
                    "  {}  {}  agent: {}\n",
                    fmt_ts(&e["timestamp"]),
                    e["event_type"].as_str().unwrap_or("?"),
                    agent,
                ));
            }
        }
        _ => out.push_str("  none\n"),
    }
}

fn render_relays(out: &mut String, v: &Value) {
    section_line(out, "Relay instances");
    if let Some(e) = v.get("error") {
        out.push_str(&format!("  unavailable: {e}\n"));
        return;
    }
    match v["relays"].as_array() {
        Some(relays) if !relays.is_empty() => {
            for r in relays {
                out.push_str(&format!(
                    "  {}  pid {}  {}\n",
                    r["id"].as_str().unwrap_or("?"),
                    r["pid"],
                    r["cwd"].as_str().unwrap_or(""),
                ));
            }
        }
        _ => out.push_str("  none\n"),
    }
}

/// Render the merged status JSON into the human terminal view.
pub fn render_status(v: &Value) -> String {
    let mut out = String::from("rUvOS system status\n");
    render_health(&mut out, &v["health"]);
    render_swarm(&mut out, &v["swarm"]);
    render_agents(&mut out, &v["agents"]);
    render_events(&mut out, &v["events"]);
    render_relays(&mut out, &v["relays"]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_handles_empty_and_populated_sections() {
        let v = serde_json::json!({
            "health": {
                "status": "ok",
                "version": "4.0.0-rc.1",
                "pid": 1234,
                "data_root": "/tmp/ruvos-home",
                "tool_count": 50,
                "persisted": {"sessions": 2, "memory_entries": 7, "agents": 1, "intel_patterns": 0},
            },
            "swarm": {"exists": false, "status": "inactive"},
            "agents": {"count": 0, "agents": []},
            "events": {"count": 1, "events": [
                {"id": "ev-1", "event_type": "swarm.created", "agent_id": null,
                 "task_id": null, "payload": {}, "timestamp": 1760000000}
            ]},
            "relays": {"scope": "machine", "count": 0, "relays": [], "inbox": []},
        });
        let out = render_status(&v);
        assert!(out.contains("Health"));
        assert!(out.contains("ok"));
        assert!(out.contains("no active swarm"));
        assert!(out.contains("swarm.created"));
        assert!(out.contains("Agents"));
        assert!(out.contains("Relay instances"));
    }

    #[test]
    fn render_populated_swarm_agents_and_relays() {
        let v = serde_json::json!({
            "health": {"error": "store unavailable"},
            "swarm": {"exists": true, "state": {
                "id": "swarm-1", "objective": "ship status cmd", "topology": "star",
                "coordinator": "agent-0", "max_agents": 6, "status": "active",
                "members": [{"agent_id": "agent-0", "role": "coordinator",
                             "state": "active", "assigned_tasks": ["t1", "t2"]}],
            }},
            "agents": {"count": 1, "agents": [
                {"agent_id": "agent-0", "archetype": "coder", "status": "completed",
                 "created_at": "2026-06-12T10:00:00Z", "message_count": 3}
            ]},
            "events": {"count": 0, "events": []},
            "relays": {"scope": "machine", "count": 1, "relays": [
                {"id": "peer-x", "pid": 99, "cwd": "/tmp/proj", "git_repo": null,
                 "summary": "idle", "updated_at": "2026-06-12T10:00:00Z"}
            ], "inbox": []},
        });
        let out = render_status(&v);
        assert!(out.contains("unavailable: "));
        assert!(out.contains("swarm-1"));
        assert!(out.contains("ship status cmd"));
        assert!(out.contains("2 task(s)"));
        assert!(out.contains("agent-0 [coder] completed"));
        assert!(out.contains("peer-x"));
        assert!(out.contains("/tmp/proj"));
    }

    #[tokio::test]
    async fn collect_returns_all_sections_on_empty_state() {
        let _guard = crate::commands::ruvos_home_lock().await;
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("RUVOS_HOME", dir.path());
        let v = collect_status().await.unwrap();
        assert!(v.get("health").is_some());
        assert!(v.get("swarm").is_some());
        assert!(v.get("agents").is_some());
        assert!(v.get("events").is_some());
        assert!(v.get("relays").is_some());
    }
}
