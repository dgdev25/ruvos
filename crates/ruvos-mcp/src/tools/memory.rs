//! Memory domain tools (4): search, store, retrieve, list.
//!
//! Backed by a real JSON store on disk (source of truth, survives restarts).
//! `memory.search` runs real retrieval: embeddings + a real HNSW
//! approximate-nearest-neighbour index (via `ruvector-core`), then MMR
//! re-ranking for diversity and a recency boost. See `super::embedding`.
//!
//! RuLake is wired in as a **second ranking tier** (additive). On
//! `memory.store` the entry's embedding is also appended to a
//! process-global `RuLake` / `LocalBackend` instance keyed by namespace.
//! On `memory.search` both the HNSW candidates and the RuLake results
//! are merged (union by key, max score wins) before MMR.

use super::handler::{ExecuteFuture, ToolHandler};
use crate::constants::DEFAULT_MEMORY_TOP_K;
use crate::runtime::{publish_event, RuntimeEvent};
use crate::{paths, Result, RuvosError};
use rulake::{LocalBackend, RuLake, SearchResult as LakeSearchResult};
use ruvos_memory_graph::MemoryGraph;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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

// ── RuLake process-global instance ─────────────────────────────────────────
//
// A single `RuLake` + `LocalBackend` lives for the duration of the process.
// The backend id is "local"; namespace becomes the collection name.
// A companion `HashMap<u64, String>` maps the FNV-1a hash of each key back
// to the key string so we can resolve RuLake hit ids back to memory keys.

struct RuLakeState {
    lake: RuLake,
    backend: Arc<LocalBackend>,
    /// FNV-1a(key) → key  reverse map.
    id_to_key: HashMap<u64, String>,
    /// Tracks which (namespace, collection) pairs have been initialised.
    initialized_collections: std::collections::HashSet<String>,
}

lazy_static::lazy_static! {
    static ref LAKE: Mutex<RuLakeState> = {
        let backend = Arc::new(LocalBackend::new("local"));
        let lake = RuLake::new(20, 42);
        // unwrap: first registration never fails
        lake.register_backend(Arc::clone(&backend) as Arc<dyn rulake::BackendAdapter>)
            .unwrap();
        Mutex::new(RuLakeState {
            lake,
            backend,
            id_to_key: HashMap::new(),
            initialized_collections: std::collections::HashSet::new(),
        })
    };
}

// Serialize all file access within a process so concurrent tool calls don't
// clobber each other. Disk remains the source of truth.
lazy_static::lazy_static! {
    static ref FILE_LOCK: Mutex<()> = Mutex::new(());
}

// ── Temporal knowledge graph (additive second view) ─────────────────────────
//
// `memory.store` feeds each value into a persisted `MemoryGraph` (entities +
// co-occurrence edges); `memory.search` augments its results with a
// `related_entities` array drawn from the graph. The graph is best-effort: a
// graph error never fails the underlying store/search. One graph is cached per
// resolved data root so parallel tests stay isolated (mirrors `crate::store`).
lazy_static::lazy_static! {
    static ref GRAPHS: Mutex<HashMap<PathBuf, &'static Mutex<MemoryGraph>>> =
        Mutex::new(HashMap::new());
}

/// Process-global memory graph for the current data root, opened from
/// `memory-graph.json`. Returns `None` if the graph cannot be opened (in which
/// case callers silently skip the graph — it is additive).
fn graph() -> Option<&'static Mutex<MemoryGraph>> {
    let key = paths::memory_graph_file();
    let mut map = GRAPHS.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(g) = map.get(&key) {
        return Some(g);
    }
    let _ = paths::ensure_root();
    let opened = MemoryGraph::open(&key).ok()?;
    let leaked: &'static Mutex<MemoryGraph> = Box::leak(Box::new(Mutex::new(opened)));
    map.insert(key, leaked);
    Some(leaked)
}

/// FNV-1a 64-bit hash of a string key — stable, deterministic u64 id for
/// the RuLake backend. Keeps the same algorithm as `embedding::fnv1a` to
/// avoid a separate dependency.
fn key_hash(key: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in key.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Append an entry to the RuLake `LocalBackend`. Creates the collection
/// (via `put_collection`) on first use; subsequent calls use `append`.
fn lake_append(namespace: &str, key: &str, vec: Vec<f32>) {
    let id = key_hash(key);
    if let Ok(mut state) = LAKE.lock() {
        state.id_to_key.insert(id, key.to_string());
        if !state.initialized_collections.contains(namespace) {
            // Initialize with this first vector via put_collection.
            let _ = state.backend.put_collection(
                namespace,
                super::embedding::EMBED_DIM,
                vec![id],
                vec![vec],
            );
            state.initialized_collections.insert(namespace.to_string());
        } else {
            let _ = state.backend.append(namespace, id, vec);
        }
    }
}

/// Query RuLake for the top-k hits in a namespace, returning `(key, score)`.
/// Returns an empty vec on any error (RuLake is additive, not required).
fn lake_search(namespace: &str, query_vec: &[f32], k: usize) -> Vec<(String, f32)> {
    let state = match LAKE.lock() {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let hits: Vec<LakeSearchResult> = state
        .lake
        .search_one("local", namespace, query_vec, k)
        .unwrap_or_default();
    hits.into_iter()
        .filter_map(|h| state.id_to_key.get(&h.id).map(|key| (key.clone(), h.score)))
        .collect()
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

/// Record a best-effort signal in the same memory store used for regular
/// retrieval so compression outcomes can be searched like any other runtime
/// event.
pub fn record_memory_signal(
    namespace: &str,
    key: &str,
    value: &str,
    tags: &[String],
) -> Result<()> {
    let _guard = FILE_LOCK.lock().unwrap();
    let mut store = load_store();
    let entry = MemoryEntry {
        key: key.to_string(),
        value: value.to_string(),
        namespace: namespace.to_string(),
        tags: tags.to_vec(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    store
        .entry(namespace.to_string())
        .or_default()
        .insert(key.to_string(), entry);
    save_store(&store)
}

// ── Bandit reward store ─────────────────────────────────────────────────────
//
// Per-(namespace,key) Beta-Bernoulli reward, persisted as JSON alongside the
// memory store. Used to nudge ranking toward entries that proved useful.
// Keyed by "{namespace}\u{1}{key}". A missing key defaults to a neutral prior,
// so ranking is unaffected before any feedback exists.

type Rewards = BTreeMap<String, Reward>;

fn reward_key(namespace: &str, key: &str) -> String {
    format!("{namespace}\u{1}{key}")
}

fn load_rewards() -> Rewards {
    match std::fs::read(paths::memory_rewards_file()) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Rewards::new(),
    }
}

fn save_rewards(rewards: &Rewards) -> Result<()> {
    paths::ensure_root()
        .map_err(|e| RuvosError::InternalError(format!("cannot create data dir: {}", e)))?;
    let path = paths::memory_rewards_file();
    let bytes = serde_json::to_vec_pretty(rewards)
        .map_err(|e| RuvosError::InternalError(format!("serialize rewards: {}", e)))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes)
        .map_err(|e| RuvosError::InternalError(format!("write rewards: {}", e)))?;
    std::fs::rename(&tmp, &path)
        .map_err(|e| RuvosError::InternalError(format!("commit rewards: {}", e)))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Real ranking primitives
// ---------------------------------------------------------------------------

use super::embedding::{
    acorn_rank_with_observer, cosine_dense, embed, hnsw_rank_with_observer, DenseRetrievalEvent,
};
use super::retrieval::{
    bm25_rank_with_observer, retrieval_trace_event, rrf_fuse_with_observer, RetrievalEvent, Reward,
};

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
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["key", "value"],
            "properties": {
                "key": { "type": "string", "description": "Unique identifier for the entry" },
                "value": { "type": "string", "description": "Content to store" },
                "namespace": { "type": "string", "description": "Namespace for the entry", "default": "default" },
                "tags": { "type": "array", "items": { "type": "string" } }
            },
            "additionalProperties": false
        })
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
                .insert(key.clone(), entry.clone());
            save_store(&store)?;
            drop(_guard);

            // Additive: also seed RuLake with this entry's embedding.
            let vec = embed(&entry_text(&entry));
            lake_append(&namespace, &key, vec);

            publish_event(RuntimeEvent {
                kind: "memory.store.completed".to_string(),
                payload: json!({
                    "key": key.clone(),
                    "namespace": namespace.clone(),
                    "tag_count": entry.tags.len(),
                }),
                agent_id: None,
                task_id: None,
            });

            // Additive: ingest the value into the temporal knowledge graph
            // (entities + co-occurrence edges). Best-effort — never fails store.
            if let Some(g) = graph() {
                if let Ok(mut guard) = g.lock() {
                    let _ = guard.add_episode(entry.value.clone(), namespace.clone());
                }
            }

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
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["key"],
            "properties": {
                "key": { "type": "string" },
                "namespace": { "type": "string", "default": "default" }
            },
            "additionalProperties": false
        })
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
            let found = store.get(&namespace).and_then(|ns| ns.get(&key)).cloned();
            publish_event(RuntimeEvent {
                kind: "memory.retrieve.queried".to_string(),
                payload: json!({
                    "key": key.clone(),
                    "namespace": namespace.clone(),
                    "found": found.is_some(),
                }),
                agent_id: None,
                task_id: None,
            });
            match found {
                Some(e) => {
                    publish_event(RuntimeEvent {
                        kind: "memory.retrieve.completed".to_string(),
                        payload: json!({
                            "key": e.key,
                            "namespace": e.namespace,
                        }),
                        agent_id: None,
                        task_id: None,
                    });
                    Ok(json!({
                        "found": true,
                        "key": e.key,
                        "value": e.value,
                        "namespace": e.namespace,
                        "tags": e.tags,
                        "created_at": e.created_at
                    }))
                }
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

            publish_event(RuntimeEvent {
                kind: "memory.list.completed".to_string(),
                payload: json!({
                    "namespace": namespace.clone(),
                    "count": entries.len(),
                }),
                agent_id: None,
                task_id: None,
            });

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
    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["query"],
            "properties": {
                "query": { "type": "string", "description": "Natural language search query" },
                "namespace": { "type": "string", "default": "default" },
                "top_k": { "type": "integer", "default": 10 },
                "filter_tags": { "type": "array", "items": { "type": "string" } }
            },
            "additionalProperties": true
        })
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
                .unwrap_or(DEFAULT_MEMORY_TOP_K as u64) as usize;
            // Optional predicate: only return entries carrying ALL of these tags.
            let filter_tags: Vec<String> = params
                .get("filter_tags")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            publish_event(RuntimeEvent {
                kind: "memory.search.started".to_string(),
                payload: json!({
                    "query": query.clone(),
                    "namespace": namespace.clone(),
                    "top_k": top_k,
                    "filter_tag_count": filter_tags.len(),
                    "has_feedback": params.get("feedback").and_then(|v| v.as_array()).is_some(),
                }),
                agent_id: None,
                task_id: None,
            });

            let _guard = FILE_LOCK.lock().unwrap();
            let store = load_store();

            // Bandit feedback (ADR-005): `feedback:[{key, useful}]` records which
            // results proved useful, reweighting future ranking. Applied first so
            // the effect is visible within the same call.
            let mut rewards = load_rewards();
            if let Some(fb) = params.get("feedback").and_then(|v| v.as_array()) {
                let mut changed = false;
                for item in fb {
                    if let Some(key) = item.get("key").and_then(|v| v.as_str()) {
                        let useful = item.get("useful").and_then(|v| v.as_bool()).unwrap_or(true);
                        rewards
                            .entry(reward_key(&namespace, key))
                            .or_default()
                            .update(useful);
                        changed = true;
                    }
                }
                if changed {
                    save_rewards(&rewards)?;
                    publish_event(RuntimeEvent {
                        kind: "memory.search.feedback_applied".to_string(),
                        payload: json!({
                            "namespace": namespace.clone(),
                            "feedback_count": fb.len(),
                        }),
                        agent_id: None,
                        task_id: None,
                    });
                }
            }

            // All entries in the namespace, keyed for retrieval.
            let entries: Vec<MemoryEntry> = store
                .get(&namespace)
                .map(|ns| ns.values().cloned().collect())
                .unwrap_or_default();
            let by_key: BTreeMap<String, MemoryEntry> =
                entries.iter().map(|e| (e.key.clone(), e.clone())).collect();

            // Tag predicate over the candidate set. `true` for every entry when
            // no `filter_tags` are given (the search then behaves as before).
            let passes_filter =
                |e: &MemoryEntry| -> bool { filter_tags.iter().all(|t| e.tags.contains(t)) };

            // ── Tier 1: ANN retrieval ──────────────────────────────────
            // Pull a wider candidate set so MMR has room to diversify.
            let items: Vec<(String, String)> = entries
                .iter()
                .map(|e| (e.key.clone(), entry_text(e)))
                .collect();
            let candidate_k = (top_k * 4).max(top_k);
            // With a tag filter, route through ACORN (predicate-agnostic
            // filtered HNSW): it keeps recall high at low selectivity, where
            // post-filtering plain-HNSW candidates would return too few hits.
            // Without a filter, the plain HNSW path is unchanged.
            let dense_ids = if filter_tags.is_empty() {
                hnsw_rank_with_observer(&items, &query, candidate_k, |event| match event {
                    DenseRetrievalEvent::HnswStarted {
                        item_count,
                        query,
                        limit,
                    } => publish_event(retrieval_trace_event(
                        "retrieval.hnsw.started",
                        "hnsw",
                        "started",
                        json!({
                            "item_count": item_count,
                            "query": query,
                            "limit": limit,
                        }),
                    )),
                    DenseRetrievalEvent::HnswCompleted { result_count } => {
                        publish_event(retrieval_trace_event(
                            "retrieval.hnsw.completed",
                            "hnsw",
                            "completed",
                            json!({
                                "result_count": result_count,
                            }),
                        ))
                    }
                    DenseRetrievalEvent::AcornStarted { .. }
                    | DenseRetrievalEvent::AcornCompleted { .. }
                    | DenseRetrievalEvent::RabitqStarted { .. }
                    | DenseRetrievalEvent::RabitqCompleted { .. } => {}
                })
                .map_err(|e| RuvosError::InternalError(format!("hnsw search: {}", e)))?
            } else {
                let keep = |pos: usize| entries.get(pos).map(&passes_filter).unwrap_or(false);
                acorn_rank_with_observer(&items, &query, candidate_k, &keep, |event| match event {
                    DenseRetrievalEvent::AcornStarted {
                        item_count,
                        query,
                        limit,
                    } => publish_event(retrieval_trace_event(
                        "retrieval.acorn.started",
                        "acorn",
                        "started",
                        json!({
                            "item_count": item_count,
                            "query": query,
                            "limit": limit,
                        }),
                    )),
                    DenseRetrievalEvent::AcornCompleted { result_count } => {
                        publish_event(retrieval_trace_event(
                            "retrieval.acorn.completed",
                            "acorn",
                            "completed",
                            json!({
                                "result_count": result_count,
                            }),
                        ))
                    }
                    DenseRetrievalEvent::HnswStarted { .. }
                    | DenseRetrievalEvent::HnswCompleted { .. }
                    | DenseRetrievalEvent::RabitqStarted { .. }
                    | DenseRetrievalEvent::RabitqCompleted { .. } => {}
                })
                .map_err(|e| RuvosError::InternalError(format!("acorn search: {}", e)))?
            };

            // Hybrid: fuse the dense ranking with a sparse BM25 ranking (ADR-005).
            // BM25 catches exact/rare-term matches dense vectors miss; under a tag
            // filter it runs over the matching subset so it can't surface non-hits.
            let bm25_items: Vec<(String, String)> = if filter_tags.is_empty() {
                items.clone()
            } else {
                entries
                    .iter()
                    .filter(|e| passes_filter(e))
                    .map(|e| (e.key.clone(), entry_text(e)))
                    .collect()
            };
            let bm25_ids =
                bm25_rank_with_observer(&bm25_items, &query, candidate_k, |event| match event {
                    RetrievalEvent::Bm25Started {
                        query,
                        doc_count,
                        limit,
                    } => publish_event(retrieval_trace_event(
                        "retrieval.bm25.started",
                        "bm25",
                        "started",
                        json!({
                            "query": query,
                            "doc_count": doc_count,
                            "limit": limit,
                        }),
                    )),
                    RetrievalEvent::Bm25Scored {
                        query_term_count,
                        doc_count,
                    } => publish_event(retrieval_trace_event(
                        "retrieval.bm25.scored",
                        "bm25",
                        "scored",
                        json!({
                            "query_term_count": query_term_count,
                            "doc_count": doc_count,
                        }),
                    )),
                    RetrievalEvent::Bm25Completed { result_count } => {
                        publish_event(retrieval_trace_event(
                            "retrieval.bm25.completed",
                            "bm25",
                            "completed",
                            json!({
                                "result_count": result_count,
                            }),
                        ))
                    }
                    RetrievalEvent::RrfCompleted { .. } => {}
                });
            let ranked_ids =
                rrf_fuse_with_observer(&[&dense_ids, &bm25_ids], candidate_k, |event| {
                    if let RetrievalEvent::RrfCompleted {
                        ranking_count,
                        result_count,
                    } = event
                    {
                        publish_event(retrieval_trace_event(
                            "retrieval.rrf.completed",
                            "rrf",
                            "completed",
                            json!({
                                "ranking_count": ranking_count,
                                "result_count": result_count,
                            }),
                        ));
                    }
                });

            // Blend the (dense cosine) relevance with a recency boost, then MMR.
            let query_vec = embed(&query);

            // ── Tier 2: RuLake federated search ────────────────────────
            // Query the process-global RuLake instance for its cached/
            // compressed candidates (additive second tier).
            let lake_hits = lake_search(&namespace, &query_vec, candidate_k);

            // ── Merge: union by key, max score wins ─────────────────────
            // Start from HNSW candidates, then add any RuLake keys not
            // already present (or upgrade their score if RuLake scored
            // higher). We use a `HashMap<key, cosine_score>` for O(1)
            // dedup before building the `Scored` slice.
            let mut merged_scores: HashMap<String, f32> = HashMap::new();
            for id in &ranked_ids {
                if let Some(e) = by_key.get(id) {
                    let vec = embed(&entry_text(e));
                    let sim = cosine_dense(&query_vec, &vec);
                    merged_scores
                        .entry(id.clone())
                        .and_modify(|s| *s = s.max(sim))
                        .or_insert(sim);
                }
            }
            // RuLake scores are L2² distances (lower = closer). Convert
            // to a similarity in [0, 1] with `1 / (1 + score)` so we can
            // compare them on the same scale as cosine similarity.
            for (key, lake_score) in &lake_hits {
                // Respect the tag filter on this tier too — RuLake is unaware
                // of the predicate, so a non-matching key must not slip in.
                if by_key.get(key).is_some_and(&passes_filter) {
                    let sim = 1.0_f32 / (1.0 + lake_score);
                    merged_scores
                        .entry(key.clone())
                        .and_modify(|s| *s = s.max(sim))
                        .or_insert(sim);
                }
            }

            let mut scored: Vec<Scored> = merged_scores
                .iter()
                .filter_map(|(key, &raw_sim)| by_key.get(key).map(|e| (e, raw_sim)))
                .map(|(e, raw_sim)| {
                    let vec = embed(&entry_text(e));
                    // Bandit tilt: a neutral (un-fed-back) entry has weight 0.75,
                    // proven-useful entries approach 1.0 — biases without dominating.
                    let bandit = rewards
                        .get(&reward_key(&namespace, &e.key))
                        .copied()
                        .unwrap_or_default()
                        .weight() as f64;
                    let relevance =
                        (0.85 * raw_sim as f64 + 0.15 * recency_weight(&e.created_at)) * bandit;
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

            publish_event(RuntimeEvent {
                kind: "memory.search.completed".to_string(),
                payload: json!({
                    "query": query.clone(),
                    "namespace": namespace.clone(),
                    "result_count": results.len(),
                    "candidate_count": candidate_k,
                    "filter_tag_count": filter_tags.len(),
                }),
                agent_id: None,
                task_id: None,
            });

            // Additive: surface related entities from the temporal knowledge
            // graph. Best-effort — an empty array when the graph is unavailable.
            let related_entities: Vec<Value> = graph()
                .and_then(|g| {
                    g.lock().ok().map(|guard| {
                        guard
                            .search(&query, top_k)
                            .iter()
                            .map(|n| {
                                json!({
                                    "name": n.name,
                                    "summary": n.summary
                                })
                            })
                            .collect()
                    })
                })
                .unwrap_or_default();

            publish_event(RuntimeEvent {
                kind: "memory.search.related_entities".to_string(),
                payload: json!({
                    "query": query.clone(),
                    "namespace": namespace.clone(),
                    "related_count": related_entities.len(),
                }),
                agent_id: None,
                task_id: None,
            });

            Ok(json!({
                "query": query,
                "namespace": namespace,
                "count": results.len(),
                "results": results,
                "related_entities": related_entities
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::gov::GovEventsHandler;

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

    async fn events(event_type: &str) -> Value {
        GovEventsHandler
            .execute(json!({"event_type": event_type, "limit": 10}))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn bandit_feedback_promotes_used_entry() {
        let _g = isolate();
        // Two entries with identical text → identical relevance before feedback.
        store("win", "alpha beta gamma", "p").await;
        store("lose", "alpha beta gamma", "p").await;

        // Repeatedly tell search that "win" was useful.
        for _ in 0..6 {
            MemorySearchHandler
                .execute(json!({
                    "query": "alpha", "namespace": "p",
                    "feedback": [{"key": "win", "useful": true}]
                }))
                .await
                .unwrap();
        }
        // A plain search now ranks the repeatedly-useful entry first.
        let r = MemorySearchHandler
            .execute(json!({"query": "alpha", "namespace": "p", "top_k": 2}))
            .await
            .unwrap();
        assert_eq!(
            r["results"][0]["key"], "win",
            "bandit must promote the entry that proved useful"
        );
        // Reward table persisted to disk.
        assert!(paths::memory_rewards_file().exists());
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
    async fn store_publishes_runtime_event() {
        let _g = isolate();
        store("trace", "value", "events").await;
        let r = events("memory.store.completed").await;
        assert!(r["count"].as_u64().unwrap() >= 1);
        assert_eq!(r["events"][0]["payload"]["key"], "trace");
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
    async fn retrieve_publishes_runtime_event() {
        let _g = isolate();
        store("seen", "hello", "ns").await;
        let _ = MemoryRetrieveHandler
            .execute(json!({"key": "seen", "namespace": "ns"}))
            .await
            .unwrap();
        let r = events("memory.retrieve.completed").await;
        assert!(r["count"].as_u64().unwrap() >= 1);
        assert_eq!(r["events"][0]["payload"]["key"], "seen");
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
        let events = events("memory.list.completed").await;
        assert!(events["count"].as_u64().unwrap() >= 1);
        assert_eq!(events["events"][0]["payload"]["namespace"], "box");
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
    async fn store_builds_graph_entities() {
        let _g = isolate();
        store("e1", "Alice met Bob at the London conference", "default").await;
        // The knowledge graph should have ingested named entities.
        let g = graph().expect("graph opens");
        let guard = g.lock().unwrap();
        assert!(
            guard.node_count() > 0,
            "memory.store must build graph entities"
        );
        assert!(guard.get_entity("Alice").is_some());
    }

    #[tokio::test]
    async fn search_returns_related_entities() {
        let _g = isolate();
        store(
            "db",
            "PostgreSQL is a relational database used by engineers",
            "default",
        )
        .await;
        let r = MemorySearchHandler
            .execute(json!({"query": "database relational storage", "namespace": "default"}))
            .await
            .unwrap();
        let related = r["related_entities"].as_array().unwrap();
        assert!(
            !related.is_empty(),
            "search must surface related graph entities: {:?}",
            r
        );
        let events = events("memory.search.completed").await;
        assert!(events["count"].as_u64().unwrap() >= 1);
        assert_eq!(events["events"][0]["payload"]["namespace"], "default");
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
        let events = events("memory.search.completed").await;
        assert!(events["count"].as_u64().unwrap() >= 1);
        assert_eq!(events["events"][0]["payload"]["filter_tag_count"], 0);
    }

    #[tokio::test]
    async fn search_emits_retrieval_level_events() {
        let _g = isolate();
        store("db", "postgres database connection pooling", "default").await;
        store("ui", "react frontend button styling", "default").await;

        let _ = MemorySearchHandler
            .execute(json!({"query": "database", "namespace": "default", "top_k": 2}))
            .await
            .unwrap();

        let bm25_started = events("retrieval.bm25.started").await;
        assert!(bm25_started["count"].as_u64().unwrap() >= 1);
        assert_eq!(bm25_started["events"][0]["payload"]["algorithm"], "bm25");
        assert_eq!(bm25_started["events"][0]["payload"]["stage"], "started");
        assert_eq!(bm25_started["events"][0]["payload"]["doc_count"], 2);

        let rrf_completed = events("retrieval.rrf.completed").await;
        assert!(rrf_completed["count"].as_u64().unwrap() >= 1);
        assert_eq!(rrf_completed["events"][0]["payload"]["algorithm"], "rrf");
        assert_eq!(rrf_completed["events"][0]["payload"]["stage"], "completed");
        assert_eq!(rrf_completed["events"][0]["payload"]["ranking_count"], 2);
    }

    #[tokio::test]
    async fn search_emits_dense_retrieval_events() {
        let _g = isolate();
        store("db1", "postgres database connection pooling", "default").await;
        store("db2", "mysql indexing query planner", "default").await;

        let acorn_query = "dense-acorn-probe-7f4c";
        let _ = MemorySearchHandler
            .execute(json!({
                "query": acorn_query,
                "namespace": "default",
                "top_k": 2,
                "filter_tags": ["db"]
            }))
            .await
            .unwrap();

        let acorn_started = events("retrieval.acorn.started").await;
        assert!(acorn_started["count"].as_u64().unwrap() >= 1);
        assert_eq!(acorn_started["events"][0]["payload"]["algorithm"], "acorn");
        assert_eq!(acorn_started["events"][0]["payload"]["stage"], "started");
        assert!(
            acorn_started["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["payload"]["query"] == acorn_query),
            "expected an ACORN event for the probe query: {:?}",
            acorn_started
        );
        // The filter excludes both entries, so ACORN should see an empty subset.
        let acorn_completed = events("retrieval.acorn.completed").await;
        assert!(acorn_completed["count"].as_u64().unwrap() >= 1);

        // A plain search should still emit HNSW events.
        let hnsw_query = "dense-hnsw-probe-7f4c";
        let _ = MemorySearchHandler
            .execute(json!({"query": hnsw_query, "namespace": "default", "top_k": 2}))
            .await
            .unwrap();
        let hnsw_started = events("retrieval.hnsw.started").await;
        assert!(hnsw_started["count"].as_u64().unwrap() >= 1);
        assert_eq!(hnsw_started["events"][0]["payload"]["algorithm"], "hnsw");
        assert_eq!(hnsw_started["events"][0]["payload"]["stage"], "started");
        assert!(
            hnsw_started["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|event| event["payload"]["query"] == hnsw_query),
            "expected an HNSW event for the probe query: {:?}",
            hnsw_started
        );
        let hnsw_completed = events("retrieval.hnsw.completed").await;
        assert!(hnsw_completed["count"].as_u64().unwrap() >= 1);
    }
}
