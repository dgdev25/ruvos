//! Intel domain tools (2): pattern_search, pattern_store.
//!
//! A real, persistent trajectory store on disk. `pattern_store` appends a
//! trajectory + outcome to both the disk store (JSON, durable across restarts)
//! and a SONA `ReasoningBank` (in-memory K-means clustering, backed by the
//! `sona` substrate crate).  `pattern_search` runs both backends and merges:
//!
//! - **TF-cosine** (disk): exact keyword recall, always consistent.
//! - **SONA ReasoningBank** (in-memory): vector-space cluster similarity,
//!   finds structurally similar trajectories even when keywords differ.
//!
//! The SONA bank is re-hydrated from disk on first access so pattern clusters
//! survive process restarts (clusters re-form from stored trajectories).

use super::handler::{ExecuteFuture, ToolHandler};
use crate::{paths, Result, RuvosError};
use ruvector_sona::{PatternConfig, QueryTrajectory, ReasoningBank};
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
    /// SONA ReasoningBank — in-memory K-means clustering over trajectory embeddings.
    /// Protected by the same FILE_LOCK so store + bank updates are atomic.
    static ref SONA_BANK: Mutex<ReasoningBank> = Mutex::new(ReasoningBank::new(PatternConfig {
        embedding_dim: super::embedding::EMBED_DIM,
        k_clusters: 8,
        min_cluster_size: 1,
        quality_threshold: 0.05,
        ..PatternConfig::default()
    }));
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

/// Build a SONA `QueryTrajectory` from a stored `Pattern` for bank ingestion.
/// The trajectory embedding is the feature-hash of "trajectory_text outcome".
fn pattern_to_trajectory(p: &Pattern, idx: u64) -> QueryTrajectory {
    let text = format!("{} {}", p.trajectory.join(" "), p.outcome);
    let embedding = super::embedding::embed(&text);
    let mut traj = QueryTrajectory::new(idx, embedding);
    traj.finalize(0.8, 0);
    traj
}

/// Re-hydrate the SONA bank from the on-disk store (called after cold start or
/// whenever the bank is empty but disk has data).
fn hydrate_bank_from_disk(bank: &mut ReasoningBank, patterns: &[Pattern]) {
    if bank.trajectory_count() == 0 && !patterns.is_empty() {
        for (i, p) in patterns.iter().enumerate() {
            bank.add_trajectory(&pattern_to_trajectory(p, i as u64));
        }
        bank.extract_patterns();
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
            patterns.push(pattern.clone());
            let total = patterns.len();
            save(&patterns)?;

            // Feed the new pattern into the SONA ReasoningBank.
            let traj = pattern_to_trajectory(&pattern, (total - 1) as u64);
            if let Ok(mut bank) = SONA_BANK.lock() {
                bank.add_trajectory(&traj);
                bank.extract_patterns();
            }

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

            // --- TF-cosine pass (disk store) ---
            let mut scored: Vec<(f64, &Pattern)> = patterns
                .iter()
                .map(|p| {
                    let text = format!("{} {}", p.trajectory.join(" "), p.outcome);
                    (cosine(&query_vec, &tf(&tokenize(&text))), p)
                })
                .filter(|(s, _)| *s > 0.0)
                .collect();
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

            // --- SONA ReasoningBank pass (vector-space cluster similarity) ---
            // Re-hydrate from disk on cold start so clusters reform after restart.
            let sona_hits: std::collections::HashSet<String> =
                if let Ok(mut bank) = SONA_BANK.lock() {
                    hydrate_bank_from_disk(&mut bank, &patterns);
                    let q_embed = super::embedding::embed(&query);
                    bank.find_similar(&q_embed, top_k)
                        .iter()
                        .filter_map(|p| {
                            // Map centroid back to stored patterns by closest embedding.
                            let q = super::embedding::embed(&query);
                            let centroid = &p.centroid;
                            // Find the stored pattern whose embedding is nearest this centroid.
                            patterns
                                .iter()
                                .min_by(|a, b| {
                                    let ea = super::embedding::embed(&format!(
                                        "{} {}",
                                        a.trajectory.join(" "),
                                        a.outcome
                                    ));
                                    let eb = super::embedding::embed(&format!(
                                        "{} {}",
                                        b.trajectory.join(" "),
                                        b.outcome
                                    ));
                                    let da: f32 =
                                        ea.iter().zip(centroid).map(|(x, y)| (x - y).powi(2)).sum();
                                    let db: f32 =
                                        eb.iter().zip(centroid).map(|(x, y)| (x - y).powi(2)).sum();
                                    // Also weight by query similarity so relevant clusters win.
                                    let sa = super::embedding::cosine_dense(&ea, &q);
                                    let sb = super::embedding::cosine_dense(&eb, &q);
                                    (da - sa)
                                        .partial_cmp(&(db - sb))
                                        .unwrap_or(std::cmp::Ordering::Equal)
                                })
                                .map(|p| p.id.clone())
                        })
                        .collect()
                } else {
                    std::collections::HashSet::new()
                };

            // Merge: patterns that appear in SONA results get a +0.1 boost.
            let mut merged: Vec<(f64, &Pattern)> = patterns
                .iter()
                .map(|p| {
                    let text = format!("{} {}", p.trajectory.join(" "), p.outcome);
                    let tf_score = cosine(&query_vec, &tf(&tokenize(&text)));
                    let sona_boost = if sona_hits.contains(&p.id) { 0.1 } else { 0.0 };
                    (tf_score + sona_boost, p)
                })
                .filter(|(s, _)| *s > 0.0)
                .collect();

            // If TF returned nothing but SONA did, include SONA-only hits.
            if merged.is_empty() && !sona_hits.is_empty() {
                merged = patterns
                    .iter()
                    .filter(|p| sona_hits.contains(&p.id))
                    .map(|p| (0.1, p))
                    .collect();
            } else if merged.is_empty() {
                // Fall back to pure TF results (may be empty).
                merged = scored;
            }

            merged.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            merged.truncate(top_k);

            let results: Vec<Value> = merged
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
