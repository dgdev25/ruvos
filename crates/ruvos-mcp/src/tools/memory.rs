//! Memory domain tools (4): search, store, retrieve, list
//!
//! Phase 5v1 implementation with in-memory storage.
//! Real HNSW integration deferred to Phase 5 refinement.

use super::handler::{ExecuteFuture, ToolHandler};
use crate::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub key: String,
    pub value: String,
    pub namespace: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    pub created_at: String,
}

// In-memory storage: namespace -> key -> entry
type MemoryStore = Arc<RwLock<HashMap<String, HashMap<String, MemoryEntry>>>>;

// Global memory store instance (Phase 5v1 only; Phase 5+ will use SQLite)
lazy_static::lazy_static! {
    static ref MEMORY_STORE: MemoryStore = Arc::new(RwLock::new(HashMap::new()));
}

// ============================================================================
// memory.search handler
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
            return Err(crate::RuvosError::InvalidParams(
                "params must be an object".to_string(),
            ));
        }

        if params.get("query").and_then(|v| v.as_str()).is_none() {
            return Err(crate::RuvosError::InvalidParams(
                "missing 'query' field (string)".to_string(),
            ));
        }

        if params.get("namespace").and_then(|v| v.as_str()).is_none() {
            return Err(crate::RuvosError::InvalidParams(
                "missing 'namespace' field (string)".to_string(),
            ));
        }

        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let query = params
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap()
                .to_string();
            let namespace = params
                .get("namespace")
                .and_then(|v| v.as_str())
                .unwrap()
                .to_string();
            let top_k = params.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

            let store = MEMORY_STORE.read().unwrap();
            let mut results = Vec::new();

            if let Some(ns_entries) = store.get(&namespace) {
                // Phase 5v1: simple substring matching (Phase 5+ uses HNSW semantic search)
                for (_, entry) in ns_entries.iter() {
                    if entry.value.to_lowercase().contains(&query.to_lowercase())
                        || entry.key.to_lowercase().contains(&query.to_lowercase())
                    {
                        results.push(json!({
                            "key": entry.key,
                            "value": entry.value,
                            "namespace": entry.namespace,
                            "score": 1.0,
                            "tags": entry.tags,
                            "created_at": entry.created_at,
                        }));
                    }
                }
            }

            results.truncate(top_k);

            Ok(json!({
                "query": query,
                "namespace": namespace,
                "top_k": top_k,
                "count": results.len(),
                "results": results,
            }))
        })
    }
}

// ============================================================================
// memory.store handler
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
        if !params.is_object() {
            return Err(crate::RuvosError::InvalidParams(
                "params must be an object".to_string(),
            ));
        }

        if params.get("key").and_then(|v| v.as_str()).is_none() {
            return Err(crate::RuvosError::InvalidParams(
                "missing 'key' field (string)".to_string(),
            ));
        }

        if params.get("value").and_then(|v| v.as_str()).is_none() {
            return Err(crate::RuvosError::InvalidParams(
                "missing 'value' field (string)".to_string(),
            ));
        }

        if params.get("namespace").and_then(|v| v.as_str()).is_none() {
            return Err(crate::RuvosError::InvalidParams(
                "missing 'namespace' field (string)".to_string(),
            ));
        }

        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let key = params
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap()
                .to_string();
            let value = params
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap()
                .to_string();
            let namespace = params
                .get("namespace")
                .and_then(|v| v.as_str())
                .unwrap()
                .to_string();

            let tags = params.get("tags").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            });

            let embedding = params
                .get("embedding")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_f64().map(|f| f as f32))
                        .collect()
                });

            let entry = MemoryEntry {
                key: key.clone(),
                value,
                namespace: namespace.clone(),
                tags,
                embedding,
                created_at: Utc::now().to_rfc3339(),
            };

            let mut store = MEMORY_STORE.write().unwrap();
            store
                .entry(namespace.clone())
                .or_default()
                .insert(key.clone(), entry);

            Ok(json!({
                "status": "stored",
                "key": key,
                "namespace": namespace,
                "timestamp": Utc::now().to_rfc3339(),
            }))
        })
    }
}

// ============================================================================
// memory.retrieve handler
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
        if !params.is_object() {
            return Err(crate::RuvosError::InvalidParams(
                "params must be an object".to_string(),
            ));
        }

        if params.get("key").and_then(|v| v.as_str()).is_none() {
            return Err(crate::RuvosError::InvalidParams(
                "missing 'key' field (string)".to_string(),
            ));
        }

        if params.get("namespace").and_then(|v| v.as_str()).is_none() {
            return Err(crate::RuvosError::InvalidParams(
                "missing 'namespace' field (string)".to_string(),
            ));
        }

        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let key = params
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap()
                .to_string();
            let namespace = params
                .get("namespace")
                .and_then(|v| v.as_str())
                .unwrap()
                .to_string();

            let store = MEMORY_STORE.read().unwrap();
            let entry = store.get(&namespace).and_then(|ns| ns.get(&key)).cloned();

            Ok(json!({
                "key": key,
                "namespace": namespace,
                "entry": entry,
                "found": entry.is_some(),
            }))
        })
    }
}

// ============================================================================
// memory.list handler
// ============================================================================

pub struct MemoryListHandler;

impl ToolHandler for MemoryListHandler {
    fn name(&self) -> &'static str {
        "list"
    }

    fn domain(&self) -> &'static str {
        "memory"
    }

    fn validate(&self, params: &Value) -> Result<()> {
        if !params.is_object() {
            return Err(crate::RuvosError::InvalidParams(
                "params must be an object".to_string(),
            ));
        }

        if params.get("namespace").and_then(|v| v.as_str()).is_none() {
            return Err(crate::RuvosError::InvalidParams(
                "missing 'namespace' field (string)".to_string(),
            ));
        }

        Ok(())
    }

    fn execute(&self, params: Value) -> ExecuteFuture {
        Box::pin(async move {
            let namespace = params
                .get("namespace")
                .and_then(|v| v.as_str())
                .unwrap()
                .to_string();

            let tags_filter = params
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>());

            let store = MEMORY_STORE.read().unwrap();
            let mut entries = Vec::new();

            if let Some(ns_entries) = store.get(&namespace) {
                for (_, entry) in ns_entries.iter() {
                    // Filter by tags if provided
                    if let Some(ref filter_tags) = tags_filter {
                        if let Some(ref entry_tags) = entry.tags {
                            let matches = filter_tags
                                .iter()
                                .any(|tag| entry_tags.contains(&tag.to_string()));
                            if matches {
                                entries.push(json!(entry));
                            }
                        }
                    } else {
                        entries.push(json!(entry));
                    }
                }
            }

            Ok(json!({
                "namespace": namespace,
                "count": entries.len(),
                "entries": entries,
            }))
        })
    }
}
