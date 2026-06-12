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

/// Max list entries rendered per section before truncation kicks in.
const MAX_LIST_ENTRIES: usize = 15;

/// Print the section header; when the section carries an error marker, also
/// print the unavailable line and return false so the renderer early-returns.
fn begin_section(out: &mut String, title: &str, v: &Value) -> bool {
    out.push_str(&format!("\n── {title} ──────────────────────────────\n"));
    if let Some(e) = v.get("error") {
        out.push_str(&format!("  unavailable: {e}\n"));
        return false;
    }
    true
}

/// Format a JSON value as an integer, falling back to "?" for non-numbers.
fn fmt_num(v: &Value) -> String {
    if let Some(n) = v.as_i64() {
        n.to_string()
    } else if let Some(n) = v.as_u64() {
        n.to_string()
    } else {
        "?".to_string()
    }
}

/// Format a unix-seconds timestamp as RFC 3339 UTC (best-effort).
/// Falls back to "-" for null/missing timestamps.
fn fmt_ts(v: &Value) -> String {
    match v
        .as_i64()
        .and_then(|s| chrono::DateTime::from_timestamp(s, 0))
    {
        Some(dt) => dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        None => v
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or("-")
            .to_string(),
    }
}

/// Append the truncation marker when a list was capped at MAX_LIST_ENTRIES.
fn truncation_line(out: &mut String, total: usize) {
    if total > MAX_LIST_ENTRIES {
        out.push_str(&format!(
            "  … and {} more (use --json for all)\n",
            total - MAX_LIST_ENTRIES
        ));
    }
}

fn render_health(out: &mut String, v: &Value) {
    if !begin_section(out, "Health", v) {
        return;
    }
    out.push_str(&format!(
        "  status: {}  version: {}  pid: {}  tools: {}\n  data root: {}\n",
        v["status"].as_str().unwrap_or("?"),
        v["version"].as_str().unwrap_or("?"),
        fmt_num(&v["pid"]),
        fmt_num(&v["tool_count"]),
        v["data_root"].as_str().unwrap_or("?"),
    ));
    if let Some(p) = v["persisted"].as_object() {
        let count = |key: &str| fmt_num(p.get(key).unwrap_or(&Value::Null));
        out.push_str(&format!(
            "  persisted: {} session(s), {} memory entr(ies), {} agent(s), {} intel pattern(s)\n",
            count("sessions"),
            count("memory_entries"),
            count("agents"),
            count("intel_patterns"),
        ));
    }
}

fn render_swarm(out: &mut String, v: &Value) {
    if !begin_section(out, "Swarm", v) {
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
        for m in members.iter().take(MAX_LIST_ENTRIES) {
            out.push_str(&format!(
                "    {} [{}] {} — {} task(s)\n",
                m["agent_id"].as_str().unwrap_or("?"),
                m["role"].as_str().unwrap_or("?"),
                m["state"].as_str().unwrap_or("?"),
                m["assigned_tasks"].as_array().map(|a| a.len()).unwrap_or(0),
            ));
        }
        truncation_line(out, members.len());
    }
}

fn render_agents(out: &mut String, v: &Value) {
    if !begin_section(out, "Agents", v) {
        return;
    }
    match v["agents"].as_array() {
        Some(agents) if !agents.is_empty() => {
            // Active agents first; stable sort preserves order otherwise.
            let mut sorted: Vec<&Value> = agents.iter().collect();
            sorted.sort_by_key(|a| match a["status"].as_str() {
                Some("running") | Some("active") => 0,
                _ => 1,
            });
            for a in sorted.iter().take(MAX_LIST_ENTRIES) {
                out.push_str(&format!(
                    "  {} [{}] {} — {} message(s), created {}\n",
                    a["agent_id"].as_str().unwrap_or("?"),
                    a["archetype"].as_str().unwrap_or("?"),
                    a["status"].as_str().unwrap_or("?"),
                    fmt_num(&a["message_count"]),
                    a["created_at"].as_str().unwrap_or("?"),
                ));
            }
            truncation_line(out, sorted.len());
        }
        _ => out.push_str("  none\n"),
    }
}

fn render_events(out: &mut String, v: &Value) {
    if !begin_section(out, "Recent events", v) {
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
    if !begin_section(out, "Relay instances", v) {
        return;
    }
    match v["relays"].as_array() {
        Some(relays) if !relays.is_empty() => {
            for r in relays {
                out.push_str(&format!(
                    "  {}  pid {}  {}\n",
                    r["id"].as_str().unwrap_or("?"),
                    fmt_num(&r["pid"]),
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
        // timestamp 1760000000 is 2025-10-09T07:33:20Z — pin the RFC 3339 prefix
        assert!(out.contains("2025-10-09T"));
        assert!(out.contains("Agents"));
        assert!(out.contains("Relay instances"));
    }

    #[test]
    fn fmt_ts_falls_back_to_dash_for_null_and_missing() {
        assert_eq!(fmt_ts(&Value::Null), "-");
        assert_eq!(fmt_ts(&serde_json::json!("")), "-");
        assert_eq!(fmt_ts(&serde_json::json!("2026-06-12")), "2026-06-12");
        assert_eq!(
            fmt_ts(&serde_json::json!(1760000000)),
            "2025-10-09T08:53:20Z"
        );
    }

    #[test]
    fn fmt_num_falls_back_to_question_mark() {
        assert_eq!(fmt_num(&serde_json::json!(42)), "42");
        assert_eq!(fmt_num(&serde_json::json!(u64::MAX)), u64::MAX.to_string());
        assert_eq!(fmt_num(&Value::Null), "?");
        assert_eq!(fmt_num(&serde_json::json!("nope")), "?");
    }

    #[test]
    fn render_truncates_long_agent_list_active_first() {
        let mut agents: Vec<Value> = (0..18)
            .map(|i| {
                serde_json::json!({
                    "agent_id": format!("agent-{i}"), "archetype": "coder",
                    "status": "completed", "created_at": "2026-06-12T10:00:00Z",
                    "message_count": i,
                })
            })
            .collect();
        agents.push(serde_json::json!({
            "agent_id": "agent-live", "archetype": "tester", "status": "running",
            "created_at": "2026-06-12T11:00:00Z", "message_count": 1,
        }));
        let members: Vec<Value> = (0..20)
            .map(|i| {
                serde_json::json!({
                    "agent_id": format!("m-{i}"), "role": "worker",
                    "state": "active", "assigned_tasks": [],
                })
            })
            .collect();
        let v = serde_json::json!({
            "health": {"error": "down"},
            "swarm": {"exists": true, "state": {
                "id": "swarm-1", "objective": "obj", "topology": "mesh",
                "status": "active", "members": members,
            }},
            "agents": {"count": agents.len(), "agents": agents},
            "events": {"count": 0, "events": []},
            "relays": {"count": 0, "relays": [], "inbox": []},
        });
        let out = render_status(&v);
        // 19 agents → 15 shown + 4 truncated; active agent sorts first.
        assert!(out.contains("  … and 4 more (use --json for all)\n"));
        assert!(out.contains("agent-live [tester] running"));
        assert!(!out.contains("agent-17"));
        // 20 members → 15 shown + 5 truncated.
        assert!(out.contains("  … and 5 more (use --json for all)\n"));
        assert!(out.contains("m-14 [worker]"));
        assert!(!out.contains("m-15 [worker]"));
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
