//! Local invariant checks for the rUvOS workspace.
//!
//! This is intentionally opinionated: it verifies the live registry against the
//! canonical contract manifest and summarizes the persisted state visible on
//! disk, so CI and humans can catch drift before it escapes into a workflow.

use crate::commands::contracts;
use anyhow::Context;
use ruvos_mcp::{paths, store, tools};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub status: String,
    pub strict: bool,
    pub data_root: String,
    pub data_root_exists: bool,
    pub tool_count: usize,
    pub manifest_matches: bool,
    pub sessions: usize,
    pub memory_entries: usize,
    pub agents: usize,
    pub intel_patterns: usize,
    pub safety_score: f64,
    pub store_busy: bool,
    pub issues: Vec<String>,
}

pub fn inspect() -> anyhow::Result<DoctorReport> {
    let root = paths::ensure_root().context("ensuring rUvOS data root")?;
    let registry = tools::tool_registry();
    let live_manifest = contracts::manifest();
    let manifest_matches = registry.len() == live_manifest.tool_count
        && registry
            .iter()
            .zip(&live_manifest.tools)
            .all(|(tool, contract)| tool.name == contract.name && tool.domain == contract.domain);

    let sessions = std::fs::read_dir(paths::sessions_dir())
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    entry
                        .path()
                        .extension()
                        .map(|ext| ext == "rvf")
                        .unwrap_or(false)
                })
                .count()
        })
        .unwrap_or(0);
    let memory_entries = count_nested(paths::memory_file());
    let intel_patterns = count_flat(paths::intel_file());

    let (agents, store_busy) = match store::try_store() {
        Some(store) => {
            let agents = store
                .list_agents()
                .map(|records| records.len())
                .unwrap_or(0);
            (agents, false)
        }
        None => (0, true),
    };

    let safety_score = {
        let engine = ruvos_mcp::safety::engine();
        let guard = engine
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.safety_score()
    };

    let mut issues = Vec::new();
    if !manifest_matches {
        issues
            .push("live tool registry does not match the canonical contract manifest".to_string());
    }
    if store_busy {
        issues.push("redb store is busy or unavailable".to_string());
    }
    if safety_score < 1.0 {
        issues.push(format!("safety score is below 1.0 ({safety_score:.3})"));
    }

    Ok(DoctorReport {
        status: if issues.is_empty() {
            "ok".to_string()
        } else {
            "warn".to_string()
        },
        strict: false,
        data_root: root.to_string_lossy().into_owned(),
        data_root_exists: root.exists(),
        tool_count: registry.len(),
        manifest_matches,
        sessions,
        memory_entries,
        agents,
        intel_patterns,
        safety_score,
        store_busy,
        issues,
    })
}

fn count_flat(path: std::path::PathBuf) -> usize {
    match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(serde_json::Value::Object(map)) => map.len(),
            Ok(serde_json::Value::Array(items)) => items.len(),
            _ => 0,
        },
        Err(_) => 0,
    }
}

fn count_nested(path: std::path::PathBuf) -> usize {
    match std::fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(serde_json::Value::Object(map)) => map
                .values()
                .map(|value| value.as_object().map(|o| o.len()).unwrap_or(0))
                .sum(),
            _ => 0,
        },
        Err(_) => 0,
    }
}

pub async fn doctor(json_output: bool, strict: bool) -> anyhow::Result<()> {
    let mut report = inspect()?;
    report.strict = strict;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("rUvOS doctor report");
        println!("status: {}", report.status);
        println!("data root: {}", report.data_root);
        println!("tool count: {}", report.tool_count);
        println!("manifest matches: {}", report.manifest_matches);
        println!("sessions: {}", report.sessions);
        println!("memory entries: {}", report.memory_entries);
        println!("agents: {}", report.agents);
        println!("intel patterns: {}", report.intel_patterns);
        println!("safety score: {:.3}", report.safety_score);
        if !report.issues.is_empty() {
            println!("issues:");
            for issue in &report.issues {
                println!("- {}", issue);
            }
        }
    }

    if strict && !report.issues.is_empty() {
        anyhow::bail!("doctor found {} issue(s)", report.issues.len());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspect_returns_manifest_alignment() {
        let root = std::env::temp_dir().join(format!("ruvos-doctor-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("temp root");
        std::env::set_var("RUVOS_HOME", &root);
        let report = inspect().expect("doctor report");
        assert!(report.tool_count >= 20);
        assert!(report.data_root_exists);
        assert!(report.manifest_matches);
        assert!(!report.store_busy);
    }
}
