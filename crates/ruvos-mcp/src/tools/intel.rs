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
use crate::{
    constants::{
        DEFAULT_INTEL_TOP_K, DEFAULT_INTENT_TOP_K, INTEL_KIND_BOOST, INTEL_SONA_BOOST,
        INTEL_SONA_K_CLUSTERS, INTEL_SONA_MIN_CLUSTER_SIZE, INTEL_SONA_QUALITY_THRESHOLD,
        INTENT_CONFIDENCE_SCALE, INTENT_TAG_OVERLAP_BOOST, SWARM_TRAJECTORY_FINALIZE_SCORE,
    },
    paths, Result, RuvosError,
};
use ruvector_sona::{PatternConfig, QueryTrajectory, ReasoningBank};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: String,
    pub trajectory: Vec<String>,
    pub outcome: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentRecord {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub tags: Vec<String>,
    pub source: String,
    pub confidence: f64,
    pub created_at: String,
}

lazy_static::lazy_static! {
    static ref FILE_LOCK: Mutex<()> = Mutex::new(());
    /// SONA ReasoningBank — in-memory K-means clustering over trajectory embeddings.
    /// Protected by the same FILE_LOCK so store + bank updates are atomic.
    static ref SONA_BANK: Mutex<ReasoningBank> = Mutex::new(ReasoningBank::new(PatternConfig {
        embedding_dim: super::embedding::EMBED_DIM,
        k_clusters: INTEL_SONA_K_CLUSTERS,
        min_cluster_size: INTEL_SONA_MIN_CLUSTER_SIZE,
        quality_threshold: INTEL_SONA_QUALITY_THRESHOLD,
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

fn load_intents() -> Vec<IntentRecord> {
    match fs::read(paths::intent_file()) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_intents(intents: &[IntentRecord]) -> Result<()> {
    paths::ensure_root().map_err(|e| RuvosError::InternalError(format!("data dir: {}", e)))?;
    let path = paths::intent_file();
    let bytes = serde_json::to_vec_pretty(intents)
        .map_err(|e| RuvosError::InternalError(format!("serialize intents: {}", e)))?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &bytes)
        .map_err(|e| RuvosError::InternalError(format!("write intents: {}", e)))?;
    fs::rename(&tmp, &path)
        .map_err(|e| RuvosError::InternalError(format!("commit intents: {}", e)))?;
    Ok(())
}

/// Record a best-effort intent signal in the same store used for stable
/// runtime preferences and patterns.
pub fn record_intent_signal(
    kind: &str,
    text: &str,
    tags: &[String],
    source: &str,
    confidence: f64,
) -> Result<()> {
    let _guard = FILE_LOCK.lock().unwrap();
    let mut intents = load_intents();
    intents.push(IntentRecord {
        id: Uuid::new_v4().to_string(),
        kind: kind.to_string(),
        text: text.to_string(),
        tags: tags.to_vec(),
        source: source.to_string(),
        confidence: confidence.clamp(0.0, 1.0),
        created_at: chrono::Utc::now().to_rfc3339(),
    });
    save_intents(&intents)
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

fn line_count(path: &Path) -> usize {
    fs::read_to_string(path)
        .map(|content| content.lines().count())
        .unwrap_or(0)
}

fn test_count(path: &Path) -> usize {
    fs::read_to_string(path)
        .map(|content| {
            content.matches("#[test]").count() + content.matches("#[tokio::test]").count()
        })
        .unwrap_or(0)
}

fn should_skip_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("target" | ".git" | ".ruvos" | "node_modules" | "dist" | "coverage")
    )
}

fn collect_files(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if !should_skip_dir(&path) {
                collect_files(&path, files);
            }
        } else if path.is_file() {
            files.push(path);
        }
    }
}

fn repo_insight_snapshot(root: &Path) -> Value {
    let mut files = Vec::new();
    collect_files(root, &mut files);

    let rust_files: Vec<PathBuf> = files
        .iter()
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("rs"))
        .cloned()
        .collect();
    let cargo_manifests = files
        .iter()
        .filter(|path| path.file_name().and_then(|name| name.to_str()) == Some("Cargo.toml"))
        .count();

    let mut crate_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut hot_files: Vec<(usize, usize, String)> = Vec::new();
    for file in &rust_files {
        let relative = file.strip_prefix(root).unwrap_or(file);
        let first_dir = relative
            .components()
            .next()
            .and_then(|component| component.as_os_str().to_str())
            .unwrap_or(".")
            .to_string();
        *crate_counts.entry(first_dir).or_insert(0) += 1;

        let lines = line_count(file);
        let tests = test_count(file);
        hot_files.push((tests, lines, relative.to_string_lossy().into_owned()));
    }

    hot_files.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| a.2.cmp(&b.2))
    });

    let test_gap_candidates: Vec<Value> = hot_files
        .iter()
        .filter(|(tests, lines, _)| *tests == 0 && *lines >= 200)
        .take(5)
        .map(|(_, lines, path)| json!({ "path": path, "lines": lines }))
        .collect();

    let hotspot_files: Vec<Value> = hot_files
        .iter()
        .take(5)
        .map(|(tests, lines, path)| json!({ "path": path, "tests": tests, "lines": lines }))
        .collect();

    let tool_domains = {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for tool in crate::tools::tool_registry() {
            *counts.entry(tool.domain).or_insert(0) += 1;
        }
        counts
    };

    json!({
        "root": root.to_string_lossy(),
        "summary": {
            "rust_files": rust_files.len(),
            "cargo_manifests": cargo_manifests,
            "tool_count": crate::tools::tool_registry().len(),
            "domains": tool_domains,
        },
        "hotspots": hotspot_files,
        "test_gap_candidates": test_gap_candidates,
        "crate_file_counts": crate_counts,
    })
}

/// Build a SONA `QueryTrajectory` from a stored `Pattern` for bank ingestion.
/// The trajectory embedding is the feature-hash of "trajectory_text outcome".
fn pattern_to_trajectory(p: &Pattern, idx: u64) -> QueryTrajectory {
    let text = format!("{} {}", p.trajectory.join(" "), p.outcome);
    let embedding = super::embedding::embed(&text);
    let mut traj = QueryTrajectory::new(idx, embedding);
    traj.finalize(SWARM_TRAJECTORY_FINALIZE_SCORE, 0);
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
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["trajectory", "outcome"],
            "properties": {
                "trajectory": { "type": "array", "items": { "type": "string" }, "description": "Ordered list of steps taken" },
                "outcome": { "type": "string", "description": "Result or lesson from the trajectory" }
            },
            "additionalProperties": false
        })
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
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": { "type": "string", "description": "Natural language query to find similar trajectories" },
                "top_k": { "type": "integer", "description": "Maximum results to return", "default": 5 }
            },
            "additionalProperties": false
        })
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
            let top_k = params
                .get("top_k")
                .and_then(|v| v.as_u64())
                .unwrap_or(DEFAULT_INTEL_TOP_K as u64) as usize;
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

            // Merge: patterns that appear in SONA results get a small boost.
            let mut merged: Vec<(f64, &Pattern)> = patterns
                .iter()
                .map(|p| {
                    let text = format!("{} {}", p.trajectory.join(" "), p.outcome);
                    let tf_score = cosine(&query_vec, &tf(&tokenize(&text)));
                    let sona_boost = if sona_hits.contains(&p.id) {
                        INTEL_SONA_BOOST
                    } else {
                        0.0
                    };
                    (tf_score + sona_boost, p)
                })
                .filter(|(s, _)| *s > 0.0)
                .collect();

            // If TF returned nothing but SONA did, include SONA-only hits.
            if merged.is_empty() && !sona_hits.is_empty() {
                merged = patterns
                    .iter()
                    .filter(|p| sona_hits.contains(&p.id))
                    .map(|p| (INTEL_SONA_BOOST, p))
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

// ============================================================================
// intel.intent_store
// ============================================================================

pub struct IntelIntentStoreHandler;

impl ToolHandler for IntelIntentStoreHandler {
    fn name(&self) -> &'static str {
        "intent_store"
    }

    fn domain(&self) -> &'static str {
        "intel"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["kind", "text"],
            "properties": {
                "kind": { "type": "string", "description": "Category: goal | preference | workflow" },
                "text": { "type": "string", "description": "Human-readable intent description" },
                "tags": { "type": "array", "items": { "type": "string" } },
                "source": { "type": "string", "default": "user" },
                "confidence": { "type": "number", "minimum": 0, "maximum": 1, "default": 1.0 }
            },
            "additionalProperties": false
        })
    }

    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("kind").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'kind' field (string)".to_string(),
            ));
        }
        if params.get("text").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'text' field (string)".to_string(),
            ));
        }
        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let kind = params["kind"].as_str().unwrap_or_default().to_string();
            let text = params["text"].as_str().unwrap_or_default().to_string();
            let source = params
                .get("source")
                .and_then(|value| value.as_str())
                .unwrap_or("user")
                .to_string();
            let confidence = params
                .get("confidence")
                .and_then(|value| value.as_f64())
                .unwrap_or(1.0)
                .clamp(0.0, 1.0);
            let tags: Vec<String> = params
                .get("tags")
                .and_then(|value| value.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            let record = IntentRecord {
                id: Uuid::new_v4().to_string(),
                kind,
                text,
                tags,
                source,
                confidence,
                created_at: chrono::Utc::now().to_rfc3339(),
            };

            let _guard = FILE_LOCK.lock().unwrap();
            let mut intents = load_intents();
            intents.push(record.clone());
            let total = intents.len();
            save_intents(&intents)?;

            Ok(json!({
                "status": "stored",
                "intent_id": record.id,
                "total_intents": total
            }))
        })
    }
}

// ============================================================================
// intel.intent_search
// ============================================================================

pub struct IntelIntentSearchHandler;

impl ToolHandler for IntelIntentSearchHandler {
    fn name(&self) -> &'static str {
        "intent_search"
    }

    fn domain(&self) -> &'static str {
        "intel"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": { "type": "string", "description": "Search for stored goals and preferences" },
                "top_k": { "type": "integer", "default": 5 },
                "kind": { "type": "string", "description": "Filter by kind (goal | preference | workflow)" }
            },
            "additionalProperties": false
        })
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
            let top_k = params
                .get("top_k")
                .and_then(|v| v.as_u64())
                .unwrap_or(DEFAULT_INTENT_TOP_K as u64) as usize;
            let kind_filter = params
                .get("kind")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let query_vec = tf(&tokenize(&query));
            let query_tokens = tokenize(&query);
            let query_token_set: std::collections::HashSet<String> =
                query_tokens.iter().cloned().collect();

            let _guard = FILE_LOCK.lock().unwrap();
            let intents = load_intents();

            let mut scored: Vec<(f64, &IntentRecord)> = intents
                .iter()
                .filter(|intent| {
                    kind_filter
                        .as_ref()
                        .map(|kind| &intent.kind == kind)
                        .unwrap_or(true)
                })
                .map(|intent| {
                    let text = format!("{} {} {}", intent.kind, intent.tags.join(" "), intent.text);
                    let text_score = cosine(&query_vec, &tf(&tokenize(&text)));
                    let tag_overlap = intent
                        .tags
                        .iter()
                        .filter(|tag| query_token_set.contains(&tag.to_lowercase()))
                        .count() as f64;
                    let kind_boost = if query_tokens.iter().any(|token| token == &intent.kind) {
                        INTEL_KIND_BOOST
                    } else {
                        0.0
                    };
                    (
                        text_score
                            + (tag_overlap * INTENT_TAG_OVERLAP_BOOST)
                            + kind_boost
                            + intent.confidence * INTENT_CONFIDENCE_SCALE,
                        intent,
                    )
                })
                .filter(|(score, _)| *score > 0.0)
                .collect();

            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            scored.truncate(top_k);

            let results: Vec<Value> = scored
                .iter()
                .map(|(score, intent)| {
                    json!({
                        "intent_id": intent.id,
                        "kind": intent.kind,
                        "text": intent.text,
                        "tags": intent.tags,
                        "source": intent.source,
                        "confidence": intent.confidence,
                        "created_at": intent.created_at,
                        "score": (score * 10000.0).round() / 10000.0,
                    })
                })
                .collect();

            Ok(json!({
                "query": query,
                "count": results.len(),
                "intents": results
            }))
        })
    }
}

// ============================================================================
// intel.repo_inspect
// ============================================================================

pub struct IntelRepoInspectHandler;

impl ToolHandler for IntelRepoInspectHandler {
    fn name(&self) -> &'static str {
        "repo_inspect"
    }

    fn domain(&self) -> &'static str {
        "intel"
    }

    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let root = params
                .get("root")
                .and_then(|value| value.as_str())
                .map(PathBuf::from)
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

            Ok(repo_insight_snapshot(&root))
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

    #[tokio::test]
    async fn intent_store_and_search_roundtrip() {
        let _g = isolate();
        IntelIntentStoreHandler
            .execute(json!({
                "kind": "goal",
                "text": "keep builds green",
                "tags": ["ci", "release"],
                "source": "user",
                "confidence": 0.9
            }))
            .await
            .unwrap();
        IntelIntentStoreHandler
            .execute(json!({
                "kind": "preference",
                "text": "prefer small focused patches",
                "tags": ["workflow"]
            }))
            .await
            .unwrap();

        let r = IntelIntentSearchHandler
            .execute(json!({"query": "builds green ci", "kind": "goal"}))
            .await
            .unwrap();
        let intents = r["intents"].as_array().unwrap();
        assert!(!intents.is_empty());
        assert_eq!(intents[0]["kind"], "goal");
        assert!(intents[0]["text"]
            .as_str()
            .unwrap()
            .contains("builds green"));
    }

    #[tokio::test]
    async fn repo_inspect_returns_repo_snapshot() {
        let _g = isolate();
        let r = IntelRepoInspectHandler.execute(json!({})).await.unwrap();
        assert!(r["summary"]["rust_files"].as_u64().unwrap() > 0);
        assert!(r["summary"]["tool_count"].as_u64().unwrap() >= 1);
        assert!(r["hotspots"].as_array().unwrap().len() <= 5);
    }

    #[test]
    fn validation() {
        assert!(IntelPatternStoreHandler.validate(&json!({})).is_err());
        assert!(IntelPatternSearchHandler.validate(&json!({})).is_err());
        assert!(IntelIntentStoreHandler.validate(&json!({})).is_err());
        assert!(IntelIntentSearchHandler.validate(&json!({})).is_err());
        assert!(IntelRepoInspectHandler.validate(&json!({})).is_ok());
    }
}
