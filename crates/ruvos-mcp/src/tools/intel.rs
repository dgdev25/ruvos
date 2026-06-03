//! Intel domain tools (2): pattern_search, pattern_store.
//!
//! A real, persistent trajectory store on disk. `pattern_store` appends a
//! trajectory + outcome; `pattern_search` ranks stored trajectories by
//! term-frequency cosine similarity to the query (the SONA "retrieve" phase),
//! backed by real persistence rather than a placeholder.

use super::handler::{ExecuteFuture, ToolHandler};
use crate::{paths, Result, RuvosError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: String,
    pub trajectory: Vec<String>,
    pub outcome: String,
    pub created_at: String,
}

lazy_static::lazy_static! {
    static ref FILE_LOCK: Mutex<()> = Mutex::new(());
}

fn load() -> Vec<Pattern> {
    match std::fs::read(paths::intel_file()) {
        Ok(b) => serde_json::from_slice(&b).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save(patterns: &[Pattern]) -> Result<()> {
    paths::ensure_root().map_err(|e| RuvosError::InternalError(format!("data dir: {}", e)))?;
    let path = paths::intel_file();
    let bytes = serde_json::to_vec_pretty(patterns)
        .map_err(|e| RuvosError::InternalError(format!("serialize intel: {}", e)))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes)
        .map_err(|e| RuvosError::InternalError(format!("write intel: {}", e)))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| RuvosError::InternalError(format!("commit intel: {}", e)))?;
    Ok(())
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

fn tf(tokens: &[String]) -> HashMap<String, f64> {
    let mut v = HashMap::new();
    for t in tokens {
        *v.entry(t.clone()).or_insert(0.0) += 1.0;
    }
    v
}

fn cosine(a: &HashMap<String, f64>, b: &HashMap<String, f64>) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let dot: f64 = a
        .iter()
        .map(|(k, va)| b.get(k).map(|vb| va * vb).unwrap_or(0.0))
        .sum();
    let na: f64 = a.values().map(|x| x * x).sum::<f64>().sqrt();
    let nb: f64 = b.values().map(|x| x * x).sum::<f64>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

// ============================================================================
// intel.pattern_store
// ============================================================================

pub struct IntelPatternStoreHandler;

impl ToolHandler for IntelPatternStoreHandler {
    fn name(&self) -> &'static str {
        "pattern_store"
    }
    fn domain(&self) -> &'static str {
        "intel"
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params
            .get("trajectory")
            .and_then(|v| v.as_array())
            .is_none()
        {
            return Err(RuvosError::InvalidParams(
                "missing 'trajectory' field (array of strings)".to_string(),
            ));
        }
        if params.get("outcome").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'outcome' field (string)".to_string(),
            ));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let trajectory: Vec<String> = params["trajectory"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let outcome = params["outcome"].as_str().unwrap_or_default().to_string();
            let id = Uuid::new_v4().to_string();

            let pattern = Pattern {
                id: id.clone(),
                trajectory,
                outcome,
                created_at: chrono::Utc::now().to_rfc3339(),
            };

            let _guard = FILE_LOCK.lock().unwrap();
            let mut patterns = load();
            patterns.push(pattern);
            let total = patterns.len();
            save(&patterns)?;

            Ok(json!({ "status": "stored", "pattern_id": id, "total_patterns": total }))
        })
    }
}

// ============================================================================
// intel.pattern_search
// ============================================================================

pub struct IntelPatternSearchHandler;

impl ToolHandler for IntelPatternSearchHandler {
    fn name(&self) -> &'static str {
        "pattern_search"
    }
    fn domain(&self) -> &'static str {
        "intel"
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("query").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'query' field (string)".to_string(),
            ));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let query = params["query"].as_str().unwrap_or_default().to_string();
            let top_k = params.get("top_k").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
            let query_vec = tf(&tokenize(&query));

            let _guard = FILE_LOCK.lock().unwrap();
            let patterns = load();

            let mut scored: Vec<(f64, &Pattern)> = patterns
                .iter()
                .map(|p| {
                    let text = format!("{} {}", p.trajectory.join(" "), p.outcome);
                    (cosine(&query_vec, &tf(&tokenize(&text))), p)
                })
                .filter(|(s, _)| *s > 0.0)
                .collect();
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
            scored.truncate(top_k);

            let results: Vec<Value> = scored
                .iter()
                .map(|(score, p)| {
                    json!({
                        "pattern_id": p.id,
                        "trajectory": p.trajectory,
                        "outcome": p.outcome,
                        "score": (score * 10000.0).round() / 10000.0
                    })
                })
                .collect();

            Ok(json!({ "query": query, "count": results.len(), "patterns": results }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolate() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(dir.path().to_path_buf());
        dir
    }

    #[tokio::test]
    async fn store_then_search_finds_similar_trajectory() {
        let _g = isolate();
        IntelPatternStoreHandler
            .execute(json!({
                "trajectory": ["read database schema", "write migration", "run tests"],
                "outcome": "success: migration applied"
            }))
            .await
            .unwrap();
        IntelPatternStoreHandler
            .execute(json!({
                "trajectory": ["design react component", "style with css"],
                "outcome": "success: ui shipped"
            }))
            .await
            .unwrap();

        let r = IntelPatternSearchHandler
            .execute(json!({"query": "database migration schema"}))
            .await
            .unwrap();
        let patterns = r["patterns"].as_array().unwrap();
        assert!(!patterns.is_empty(), "must find a matching trajectory");
        assert!(
            patterns[0]["outcome"]
                .as_str()
                .unwrap()
                .contains("migration"),
            "most similar trajectory ranks first"
        );
    }

    #[tokio::test]
    async fn store_persists_to_disk() {
        let _g = isolate();
        IntelPatternStoreHandler
            .execute(json!({"trajectory": ["a"], "outcome": "b"}))
            .await
            .unwrap();
        assert!(paths::intel_file().exists(), "intel store must hit disk");
    }

    #[tokio::test]
    async fn search_empty_store_returns_no_patterns() {
        let _g = isolate();
        let r = IntelPatternSearchHandler
            .execute(json!({"query": "anything"}))
            .await
            .unwrap();
        assert_eq!(r["count"], 0);
    }

    #[test]
    fn validation() {
        assert!(IntelPatternStoreHandler.validate(&json!({})).is_err());
        assert!(IntelPatternSearchHandler.validate(&json!({})).is_err());
    }
}
