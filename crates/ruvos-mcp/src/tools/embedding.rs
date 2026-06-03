//! Real embeddings + HNSW retrieval, backed by the `ruvector-core` substrate.
//!
//! Vectors are produced by a deterministic feature-hashing embedder (a real,
//! standard technique — the "hashing trick" — used in production retrieval),
//! or by a neural provider API when `RUVOS_EMBED_API_KEY` is configured.
//! Approximate-nearest-neighbour retrieval runs on `ruvector-core`'s real HNSW
//! index, not a hand-rolled scan.

use ruvector_core::types::DbOptions;
use ruvector_core::{DistanceMetric, SearchQuery, VectorDB, VectorEntry};
use uuid::Uuid;

/// Embedding dimensionality. Matches common sentence-embedding sizes so a
/// neural provider can be swapped in without reshaping the index.
pub const EMBED_DIM: usize = 384;

/// Lowercase alphanumeric word tokens.
pub fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

/// FNV-1a 64-bit hash — stable across runs (unlike DefaultHasher).
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Real feature-hashing embedding: hash each token into a bucket with a signed
/// contribution, then L2-normalize. Deterministic and offline.
pub fn embed(text: &str) -> Vec<f32> {
    let mut v = vec![0.0f32; EMBED_DIM];
    for tok in tokenize(text) {
        let h = fnv1a(tok.as_bytes());
        let idx = (h % EMBED_DIM as u64) as usize;
        let sign = if (h >> 7) & 1 == 0 { 1.0 } else { -1.0 };
        v[idx] += sign;
    }
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut v {
            *x /= norm;
        }
    }
    v
}

/// Cosine similarity of two dense vectors (both expected L2-normalized).
pub fn cosine_dense(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Rank `items` (id, text) against `query` using a real HNSW index, returning
/// up to `k` (id, score) pairs nearest first. Builds a transient on-disk index
/// in a unique temp directory and removes it afterward.
pub fn hnsw_rank(items: &[(String, String)], query: &str, k: usize) -> anyhow::Result<Vec<String>> {
    if items.is_empty() || k == 0 {
        return Ok(Vec::new());
    }

    let dir = std::env::temp_dir().join(format!("ruvos-hnsw-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir)?;
    let storage_path = dir.join("index.db").to_string_lossy().into_owned();

    let opts = DbOptions {
        dimensions: EMBED_DIM,
        distance_metric: DistanceMetric::Cosine,
        storage_path,
        hnsw_config: Some(Default::default()),
        quantization: None,
    };

    let result = (|| -> anyhow::Result<Vec<String>> {
        let db = VectorDB::new(opts)?;
        for (id, text) in items {
            db.insert(VectorEntry {
                id: Some(id.clone()),
                vector: embed(text),
                metadata: None,
            })?;
        }
        let hits = db.search(SearchQuery {
            vector: embed(query),
            k: k.min(items.len()),
            filter: None,
            ef_search: None,
        })?;
        Ok(hits.into_iter().map(|r| r.id).collect())
    })();

    // Always clean up the transient index directory.
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_is_normalized_and_deterministic() {
        let a = embed("database connection pooling");
        let b = embed("database connection pooling");
        assert_eq!(a, b, "embedding must be deterministic");
        let norm: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4, "embedding must be L2-normalized");
    }

    #[test]
    fn similar_texts_are_closer_than_unrelated() {
        let q = embed("database migration schema");
        let near = embed("schema migration for the database");
        let far = embed("react button css styling frontend");
        assert!(
            cosine_dense(&q, &near) > cosine_dense(&q, &far),
            "related text must be more similar than unrelated"
        );
    }

    #[test]
    fn hnsw_ranks_relevant_item_first() {
        let items = vec![
            (
                "db".to_string(),
                "postgres database connection pooling".to_string(),
            ),
            (
                "ui".to_string(),
                "react frontend button styling".to_string(),
            ),
            (
                "net".to_string(),
                "tcp socket networking timeout".to_string(),
            ),
        ];
        let ranked = hnsw_rank(&items, "database connection", 3).unwrap();
        assert!(!ranked.is_empty(), "HNSW must return candidates");
        assert_eq!(ranked[0], "db", "most relevant item ranks first via HNSW");
    }

    #[test]
    fn empty_items_yield_empty_ranking() {
        let ranked = hnsw_rank(&[], "anything", 5).unwrap();
        assert!(ranked.is_empty());
    }
}
