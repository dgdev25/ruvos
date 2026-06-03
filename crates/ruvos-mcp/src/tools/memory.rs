//! Memory domain tools (4): search, store, retrieve, list.
//!
//! Backed by a real JSON store on disk (source of truth, survives restarts).
//! `memory.search` implements genuine retrieval: term-frequency cosine
//! similarity against the query, MMR re-ranking for diversity, and a recency
//! boost — no neural embeddings required, but a real ranking algorithm.

use super::handler::{ExecuteFuture, ToolHandler};
use crate::{paths, Result, RuvosError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub key: String,
    pub value: String,
    pub namespace: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: String,
}

/// On-disk shape: namespace -> key -> entry.
type Store = BTreeMap<String, BTreeMap<String, MemoryEntry>>;

// Serialize all file access within a process so concurrent tool calls don't
// clobber each other. Disk remains the source of truth.
lazy_static::lazy_static! {
    static ref FILE_LOCK: Mutex<()> = Mutex::new(());
}

fn load_store() -> Store {
    let path = paths::memory_file();
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Store::new(),
    }
}

fn save_store(store: &Store) -> Result<()> {
    paths::ensure_root()
        .map_err(|e| RuvosError::InternalError(format!("cannot create data dir: {}", e)))?;
    let path = paths::memory_file();
    let bytes = serde_json::to_vec_pretty(store)
        .map_err(|e| RuvosError::InternalError(format!("serialize memory: {}", e)))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes)
        .map_err(|e| RuvosError::InternalError(format!("write memory: {}", e)))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| RuvosError::InternalError(format!("commit memory: {}", e)))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Real ranking primitives
// ---------------------------------------------------------------------------

/// Lowercase alphanumeric word tokens.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Term-frequency vector.
fn tf_vector(tokens: &[String]) -> HashMap<String, f64> {
    let mut v = HashMap::new();
    for t in tokens {
        *v.entry(t.clone()).or_insert(0.0) += 1.0;
    }
    v
}

/// Cosine similarity between two term-frequency vectors. Range [0, 1].
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

/// Recency weight in [0, 1]: newer entries score higher. Half-life ~30 days.
fn recency_weight(created_at: &str) -> f64 {
    let created = match chrono::DateTime::parse_from_rfc3339(created_at) {
        Ok(d) => d.with_timezone(&chrono::Utc),
        Err(_) => return 0.5,
    };
    let age_days = (chrono::Utc::now() - created).num_seconds() as f64 / 86_400.0;
    0.5_f64.powf(age_days.max(0.0) / 30.0)
}

/// Searchable text for an entry (key + value + tags).
fn entry_text(e: &MemoryEntry) -> String {
    format!("{} {} {}", e.key, e.value, e.tags.join(" "))
}

struct Scored {
    entry: MemoryEntry,
    relevance: f64,
    vec: HashMap<String, f64>,
}

/// MMR re-ranking: balances query relevance against diversity from already
/// selected results. lambda=0.7 favors relevance while still de-duplicating.
fn mmr_select(mut candidates: Vec<Scored>, top_k: usize, lambda: f64) -> Vec<Scored> {
    let mut selected: Vec<Scored> = Vec::new();
    while !candidates.is_empty() && selected.len() < top_k {
        let mut best_idx = 0;
        let mut best_score = f64::NEG_INFINITY;
        for (i, cand) in candidates.iter().enumerate() {
            let max_sim_to_selected = selected
                .iter()
                .map(|s| cosine(&cand.vec, &s.vec))
                .fold(0.0_f64, f64::max);
            let mmr = lambda * cand.relevance - (1.0 - lambda) * max_sim_to_selected;
            if mmr > best_score {
                best_score = mmr;
                best_idx = i;
            }
        }
        selected.push(candidates.remove(best_idx));
    }
    selected
}

// ============================================================================
// memory.store
// ============================================================================

pub struct MemoryStoreHandler;

impl ToolHandler for MemoryStoreHandler {
    fn name(&self) -> &'static str {
        "store"
    }
    fn domain(&self) -> &'static str {
        "memory"
    }
    fn validate(&self, params: &Value) -> Result<()> {
        for field in ["key", "value"] {
            if params.get(field).and_then(|v| v.as_str()).is_none() {
                return Err(RuvosError::InvalidParams(format!(
                    "missing '{}' field (string)",
                    field
                )));
            }
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let key = params["key"].as_str().unwrap_or_default().to_string();
            let value = params["value"].as_str().unwrap_or_default().to_string();
            let namespace = params
                .get("namespace")
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .to_string();
            let tags = params
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|t| t.as_str().map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let entry = MemoryEntry {
                key: key.clone(),
                value,
                namespace: namespace.clone(),
                tags,
                created_at: chrono::Utc::now().to_rfc3339(),
            };

            let _guard = FILE_LOCK.lock().unwrap();
            let mut store = load_store();
            store
                .entry(namespace.clone())
                .or_default()
                .insert(key.clone(), entry);
            save_store(&store)?;

            Ok(json!({
                "status": "stored",
                "key": key,
                "namespace": namespace
            }))
        })
    }
}

// ============================================================================
// memory.retrieve
// ============================================================================

pub struct MemoryRetrieveHandler;

impl ToolHandler for MemoryRetrieveHandler {
    fn name(&self) -> &'static str {
        "retrieve"
    }
    fn domain(&self) -> &'static str {
        "memory"
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if params.get("key").and_then(|v| v.as_str()).is_none() {
            return Err(RuvosError::InvalidParams(
                "missing 'key' field (string)".to_string(),
            ));
        }
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let key = params["key"].as_str().unwrap_or_default().to_string();
            let namespace = params
                .get("namespace")
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .to_string();

            let _guard = FILE_LOCK.lock().unwrap();
            let store = load_store();
            match store.get(&namespace).and_then(|ns| ns.get(&key)) {
                Some(e) => Ok(json!({
                    "found": true,
                    "key": e.key,
                    "value": e.value,
                    "namespace": e.namespace,
                    "tags": e.tags,
                    "created_at": e.created_at
                })),
                None => Ok(json!({
                    "found": false,
                    "key": key,
                    "namespace": namespace
                })),
            }
        })
    }
}

// ============================================================================
// memory.list
// ============================================================================

pub struct MemoryListHandler;

impl ToolHandler for MemoryListHandler {
    fn name(&self) -> &'static str {
        "list"
    }
    fn domain(&self) -> &'static str {
        "memory"
    }
    fn validate(&self, _params: &Value) -> Result<()> {
        Ok(())
    }
    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let namespace = params
                .get("namespace")
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .to_string();

            let _guard = FILE_LOCK.lock().unwrap();
            let store = load_store();
            let entries: Vec<Value> = store
                .get(&namespace)
                .map(|ns| {
                    ns.values()
                        .map(|e| {
                            json!({
                                "key": e.key,
                                "value": e.value,
                                "tags": e.tags,
                                "created_at": e.created_at
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

            Ok(json!({
                "namespace": namespace,
                "count": entries.len(),
                "entries": entries
            }))
        })
    }
}

// ============================================================================
// memory.search
// ============================================================================

pub struct MemorySearchHandler;

impl ToolHandler for MemorySearchHandler {
    fn name(&self) -> &'static str {
        "search"
    }
    fn domain(&self) -> &'static str {
        "memory"
    }
    fn validate(&self, params: &Value) -> Result<()> {
        if !params.is_object() {
            return Err(RuvosError::InvalidParams(
                "params must be an object".to_string(),
            ));
        }
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
            let namespace = params
                .get("namespace")
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .to_string();
            let top_k = params
                .get("top_k")
                .or_else(|| params.get("limit"))
                .and_then(|v| v.as_u64())
                .unwrap_or(5) as usize;

            let query_vec = tf_vector(&tokenize(&query));

            let _guard = FILE_LOCK.lock().unwrap();
            let store = load_store();

            let mut scored: Vec<Scored> = store
                .get(&namespace)
                .map(|ns| {
                    ns.values()
                        .map(|e| {
                            let vec = tf_vector(&tokenize(&entry_text(e)));
                            // Relevance = lexical similarity blended with recency.
                            let sim = cosine(&query_vec, &vec);
                            let relevance = 0.85 * sim + 0.15 * recency_weight(&e.created_at);
                            Scored {
                                entry: e.clone(),
                                relevance,
                                vec,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();

            // Drop zero-relevance noise, then MMR-rank for diversity.
            scored.retain(|s| s.relevance > 0.0);
            scored.sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap());
            let selected = mmr_select(scored, top_k, 0.7);

            let results: Vec<Value> = selected
                .iter()
                .map(|s| {
                    json!({
                        "key": s.entry.key,
                        "value": s.entry.value,
                        "tags": s.entry.tags,
                        "created_at": s.entry.created_at,
                        "score": (s.relevance * 10000.0).round() / 10000.0
                    })
                })
                .collect();

            Ok(json!({
                "query": query,
                "namespace": namespace,
                "count": results.len(),
                "results": results
            }))
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

    async fn store(key: &str, value: &str, ns: &str) {
        MemoryStoreHandler
            .execute(json!({"key": key, "value": value, "namespace": ns}))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn store_persists_to_disk() {
        let _g = isolate();
        store("k1", "hello world", "default").await;
        assert!(
            paths::memory_file().exists(),
            "memory.store must write a real file"
        );
    }

    #[tokio::test]
    async fn retrieve_returns_stored_value() {
        let _g = isolate();
        store("greeting", "hello there", "ns1").await;
        let r = MemoryRetrieveHandler
            .execute(json!({"key": "greeting", "namespace": "ns1"}))
            .await
            .unwrap();
        assert_eq!(r["found"], true);
        assert_eq!(r["value"], "hello there");
    }

    #[tokio::test]
    async fn retrieve_missing_is_not_found() {
        let _g = isolate();
        let r = MemoryRetrieveHandler
            .execute(json!({"key": "nope"}))
            .await
            .unwrap();
        assert_eq!(r["found"], false);
    }

    #[tokio::test]
    async fn list_counts_namespace_entries() {
        let _g = isolate();
        store("a", "one", "box").await;
        store("b", "two", "box").await;
        let r = MemoryListHandler
            .execute(json!({"namespace": "box"}))
            .await
            .unwrap();
        assert_eq!(r["count"], 2);
    }

    #[tokio::test]
    async fn search_ranks_relevant_entry_first() {
        let _g = isolate();
        store("db", "postgres database connection pooling", "default").await;
        store("ui", "react frontend button styling", "default").await;
        store("net", "tcp socket networking timeout", "default").await;

        let r = MemorySearchHandler
            .execute(json!({"query": "database connection", "namespace": "default"}))
            .await
            .unwrap();
        let results = r["results"].as_array().unwrap();
        assert!(!results.is_empty(), "search must return matches");
        assert_eq!(
            results[0]["key"], "db",
            "most relevant entry must rank first"
        );
        // The unrelated UI entry should score below the DB entry.
        assert!(results[0]["score"].as_f64().unwrap() > 0.0);
    }

    #[tokio::test]
    async fn search_survives_reload_from_disk() {
        let _g = isolate();
        store("persisted", "vector search memory", "default").await;
        // A fresh handler reads from disk — simulates a new process.
        let r = MemorySearchHandler
            .execute(json!({"query": "vector search"}))
            .await
            .unwrap();
        assert_eq!(r["count"], 1);
        assert_eq!(r["results"][0]["key"], "persisted");
    }

    #[test]
    fn cosine_and_tokenize_are_real() {
        let a = tf_vector(&tokenize("hello world hello"));
        let b = tf_vector(&tokenize("hello world"));
        let sim = cosine(&a, &b);
        assert!(sim > 0.9 && sim <= 1.0, "similar texts score high: {}", sim);
        let c = tf_vector(&tokenize("completely different tokens"));
        assert!(cosine(&a, &c) < 0.1, "dissimilar texts score low");
    }
}
