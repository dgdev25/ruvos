//! Memory domain tools (4): search, store, retrieve, list.
//!
//! Backed by a real JSON store on disk (source of truth, survives restarts).
//! `memory.search` runs real retrieval: embeddings + a real HNSW
//! approximate-nearest-neighbour index (via `ruvector-core`), then MMR
//! re-ranking for diversity and a recency boost. See `super::embedding`.

use super::handler::{ExecuteFuture, ToolHandler};
use crate::{paths, Result, RuvosError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
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

use super::embedding::{cosine_dense, embed, hnsw_rank};

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
    vec: Vec<f32>,
}

/// MMR re-ranking over real dense embeddings: balances query relevance against
/// diversity from already-selected results. lambda=0.7 favors relevance while
/// still de-duplicating near-identical hits.
fn mmr_select(mut candidates: Vec<Scored>, top_k: usize, lambda: f64) -> Vec<Scored> {
    let mut selected: Vec<Scored> = Vec::new();
    while !candidates.is_empty() && selected.len() < top_k {
        let mut best_idx = 0;
        let mut best_score = f64::NEG_INFINITY;
        for (i, cand) in candidates.iter().enumerate() {
            let max_sim_to_selected = selected
                .iter()
                .map(|s| cosine_dense(&cand.vec, &s.vec) as f64)
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

            let _guard = FILE_LOCK.lock().unwrap();
            let store = load_store();

            // All entries in the namespace, keyed for HNSW retrieval.
            let entries: Vec<MemoryEntry> = store
                .get(&namespace)
                .map(|ns| ns.values().cloned().collect())
                .unwrap_or_default();
            let by_key: BTreeMap<String, MemoryEntry> =
                entries.iter().map(|e| (e.key.clone(), e.clone())).collect();

            // Real HNSW ANN retrieval over real embeddings. Pull a wider
            // candidate set so MMR has room to diversify.
            let items: Vec<(String, String)> = entries
                .iter()
                .map(|e| (e.key.clone(), entry_text(e)))
                .collect();
            let candidate_k = (top_k * 4).max(top_k);
            let ranked_ids = hnsw_rank(&items, &query, candidate_k)
                .map_err(|e| RuvosError::InternalError(format!("hnsw search: {}", e)))?;

            // Blend the (dense cosine) relevance with a recency boost, then MMR.
            let query_vec = embed(&query);
            let mut scored: Vec<Scored> = ranked_ids
                .iter()
                .filter_map(|id| by_key.get(id))
                .map(|e| {
                    let vec = embed(&entry_text(e));
                    let sim = cosine_dense(&query_vec, &vec) as f64;
                    let relevance = 0.85 * sim + 0.15 * recency_weight(&e.created_at);
                    Scored {
                        entry: e.clone(),
                        relevance,
                        vec,
                    }
                })
                .collect();

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

    #[tokio::test]
    async fn search_diversifies_near_duplicates_via_mmr() {
        let _g = isolate();
        // Two near-identical entries + one distinct; MMR should not return
        // only the duplicates.
        store("dup1", "database connection pooling postgres", "default").await;
        store("dup2", "database connection pooling postgres", "default").await;
        store("other", "database sharding strategy", "default").await;

        let r = MemorySearchHandler
            .execute(json!({"query": "database", "namespace": "default", "top_k": 2}))
            .await
            .unwrap();
        let keys: Vec<String> = r["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["key"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(keys.len(), 2);
        assert!(
            keys.contains(&"other".to_string()),
            "MMR should surface the distinct entry, not two duplicates: {:?}",
            keys
        );
    }
}
