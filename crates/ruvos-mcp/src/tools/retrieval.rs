//! Hybrid-retrieval primitives for `memory.search` (ADR-005).
//!
//! - [`bm25_rank`] — Okapi BM25 sparse lexical ranking over the candidate set,
//!   reusing the embedding tokenizer. Catches exact/rare-term matches that dense
//!   vectors miss.
//! - [`rrf_fuse`] — Reciprocal Rank Fusion: blend several rankings (dense + BM25)
//!   into one, scale-free, before MMR/recency reranking.
//!
//! The bandit reward model ([`Reward`]) that makes ranking self-improving lives
//! here too; its persistence + wiring is applied in `memory.rs`.

use super::embedding::tokenize;
use crate::constants::{RETRIEVAL_B, RETRIEVAL_K1, RETRIEVAL_RRF_K};
use crate::runtime::RuntimeEvent;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// Build a normalized retrieval trace event payload.
pub fn retrieval_trace_event(
    kind: impl Into<String>,
    algorithm: impl Into<String>,
    stage: impl Into<String>,
    payload: Value,
) -> RuntimeEvent {
    let mut object = match payload {
        Value::Object(object) => object,
        other => {
            let mut object = Map::new();
            object.insert("detail".to_string(), other);
            object
        }
    };
    object.insert("algorithm".to_string(), Value::String(algorithm.into()));
    object.insert("stage".to_string(), Value::String(stage.into()));
    RuntimeEvent::new(kind, Value::Object(object))
}

/// Lifecycle events emitted by the ranking primitives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetrievalEvent {
    Bm25Started {
        query: String,
        doc_count: usize,
        limit: usize,
    },
    Bm25Scored {
        query_term_count: usize,
        doc_count: usize,
    },
    Bm25Completed {
        result_count: usize,
    },
    RrfCompleted {
        ranking_count: usize,
        result_count: usize,
    },
}

/// Rank `docs` (id, text) against `query` by Okapi BM25; returns up to `k` ids,
/// best-first, dropping zero-score docs. Computed over the candidate set — no
/// persistent index needed at current scale.
pub fn bm25_rank(docs: &[(String, String)], query: &str, k: usize) -> Vec<String> {
    bm25_rank_with_observer(docs, query, k, |_| {})
}

/// BM25 ranking with lifecycle observation.
pub fn bm25_rank_with_observer<F>(
    docs: &[(String, String)],
    query: &str,
    k: usize,
    mut observer: F,
) -> Vec<String>
where
    F: FnMut(&RetrievalEvent),
{
    if docs.is_empty() || k == 0 {
        return Vec::new();
    }
    observer(&RetrievalEvent::Bm25Started {
        query: query.to_string(),
        doc_count: docs.len(),
        limit: k,
    });
    let toks: Vec<Vec<String>> = docs.iter().map(|(_, t)| tokenize(t)).collect();
    let n = docs.len() as f32;
    let avgdl = (toks.iter().map(|t| t.len()).sum::<usize>() as f32 / n).max(1.0);

    // Document frequency per term (number of docs containing it).
    let mut df: HashMap<&str, f32> = HashMap::new();
    for doc in &toks {
        for term in doc.iter().collect::<HashSet<_>>() {
            *df.entry(term.as_str()).or_insert(0.0) += 1.0;
        }
    }

    let q = tokenize(query);
    observer(&RetrievalEvent::Bm25Scored {
        query_term_count: q.len(),
        doc_count: docs.len(),
    });
    let mut scored: Vec<(usize, f32)> = toks
        .iter()
        .enumerate()
        .map(|(i, doc)| {
            let dl = doc.len() as f32;
            let mut tf: HashMap<&str, f32> = HashMap::new();
            for term in doc {
                *tf.entry(term.as_str()).or_insert(0.0) += 1.0;
            }
            let score: f32 = q
                .iter()
                .map(|qt| {
                    let f = *tf.get(qt.as_str()).unwrap_or(&0.0);
                    if f == 0.0 {
                        return 0.0;
                    }
                    let n_qi = *df.get(qt.as_str()).unwrap_or(&0.0);
                    let idf = (((n - n_qi + 0.5) / (n_qi + 0.5)) + 1.0).ln();
                    idf * (f * (RETRIEVAL_K1 + 1.0))
                        / (f + RETRIEVAL_K1 * (1.0 - RETRIEVAL_B + RETRIEVAL_B * dl / avgdl))
                })
                .sum();
            (i, score)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let result: Vec<String> = scored
        .into_iter()
        .filter(|(_, s)| *s > 0.0)
        .take(k)
        .map(|(i, _)| docs[i].0.clone())
        .collect();
    observer(&RetrievalEvent::Bm25Completed {
        result_count: result.len(),
    });
    result
}

/// Reciprocal Rank Fusion of several rankings (each best-first). Scale-free:
/// `score(d) = Σ 1/(RRF_K + rank_i(d))`. Returns up to `k` ids, best-first.
pub fn rrf_fuse(rankings: &[&[String]], k: usize) -> Vec<String> {
    rrf_fuse_with_observer(rankings, k, |_| {})
}

/// RRF fusion with lifecycle observation.
pub fn rrf_fuse_with_observer<F>(rankings: &[&[String]], k: usize, mut observer: F) -> Vec<String>
where
    F: FnMut(&RetrievalEvent),
{
    let mut score: HashMap<&str, f32> = HashMap::new();
    for ranking in rankings {
        for (rank, id) in ranking.iter().enumerate() {
            *score.entry(id.as_str()).or_insert(0.0) += 1.0 / (RETRIEVAL_RRF_K + rank as f32 + 1.0);
        }
    }
    let mut v: Vec<(&str, f32)> = score.into_iter().collect();
    // Stable, deterministic: by score desc, then id asc to break ties.
    v.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(b.0))
    });
    let result: Vec<String> = v
        .into_iter()
        .take(k)
        .map(|(id, _)| id.to_string())
        .collect();
    observer(&RetrievalEvent::RrfCompleted {
        ranking_count: rankings.len(),
        result_count: result.len(),
    });
    result
}

/// Beta-Bernoulli bandit reward for a `(namespace, key)` memory entry.
/// `mean()` is the posterior probability the entry is useful; cold entries use a
/// neutral prior (α=β=1 → mean 0.5) so ranking is unaffected before any feedback.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Reward {
    pub alpha: f32,
    pub beta: f32,
}

impl Default for Reward {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            beta: 1.0,
        }
    }
}

impl Reward {
    pub fn update(&mut self, useful: bool) {
        if useful {
            self.alpha += 1.0;
        } else {
            self.beta += 1.0;
        }
    }

    /// Posterior mean in (0, 1).
    pub fn mean(&self) -> f32 {
        self.alpha / (self.alpha + self.beta)
    }

    /// Bounded ranking multiplier in [0.5, 1.0]: neutral (mean 0.5) → 0.75, so the
    /// bandit tilts ordering without dominating relevance.
    pub fn weight(&self) -> f32 {
        0.5 + 0.5 * self.mean()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn docs() -> Vec<(String, String)> {
        vec![
            ("err".into(), "error code E1042 connection refused".into()),
            (
                "db".into(),
                "postgres connection pooling via pgbouncer".into(),
            ),
            ("ui".into(), "react button styling".into()),
        ]
    }

    #[test]
    fn bm25_ranks_exact_rare_term_first() {
        let ranked = bm25_rank(&docs(), "E1042", 3);
        assert_eq!(ranked[0], "err", "exact rare-term match must rank first");
    }

    #[test]
    fn bm25_observer_emits_lifecycle_events() {
        let mut events = Vec::new();
        let ranked = bm25_rank_with_observer(&docs(), "E1042", 3, |event| {
            events.push(event.clone());
        });
        assert_eq!(ranked[0], "err");
        assert!(matches!(
            events.first(),
            Some(RetrievalEvent::Bm25Started { .. })
        ));
        assert!(events.iter().any(|event| matches!(
            event,
            RetrievalEvent::Bm25Completed { result_count } if *result_count > 0
        )));
    }

    #[test]
    fn bm25_query_term_absent_everywhere_scores_zero() {
        let ranked = bm25_rank(&docs(), "kubernetes", 3);
        assert!(ranked.is_empty(), "no doc contains the term");
    }

    #[test]
    fn bm25_empty_inputs() {
        assert!(bm25_rank(&[], "x", 5).is_empty());
        assert!(bm25_rank(&docs(), "x", 0).is_empty());
    }

    #[test]
    fn rrf_blends_both_rankings() {
        let dense = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let bm25 = vec!["c".to_string(), "a".to_string()];
        let fused = rrf_fuse(&[&dense, &bm25], 10);
        let pos = |x: &str| fused.iter().position(|y| y == x).unwrap();
        // 'a' (1st+2nd) and 'c' (3rd+1st) outrank 'b' (dense-only).
        assert!(pos("a") < pos("b"));
        assert!(pos("c") < pos("b"));
    }

    #[test]
    fn rrf_is_deterministic_on_ties() {
        let r1 = vec!["x".to_string(), "y".to_string()];
        let r2 = vec!["y".to_string(), "x".to_string()];
        // x and y tie; id-asc tiebreak makes the order deterministic.
        assert_eq!(
            rrf_fuse(&[&r1, &r2], 10),
            vec!["x".to_string(), "y".to_string()]
        );
    }

    #[test]
    fn rrf_observer_emits_completion_event() {
        let mut events = Vec::new();
        let first = vec!["a".to_string()];
        let second = vec!["b".to_string()];
        let fused = rrf_fuse_with_observer(&[&first, &second], 2, |event| {
            events.push(event.clone());
        });
        assert_eq!(fused.len(), 2);
        assert!(matches!(
            events.last(),
            Some(RetrievalEvent::RrfCompleted { .. })
        ));
    }

    #[test]
    fn retrieval_trace_event_uses_common_schema() {
        let event = retrieval_trace_event(
            "retrieval.bm25.completed",
            "bm25",
            "completed",
            serde_json::json!({"result_count": 3}),
        );
        assert_eq!(event.kind, "retrieval.bm25.completed");
        assert_eq!(event.payload["algorithm"], "bm25");
        assert_eq!(event.payload["stage"], "completed");
        assert_eq!(event.payload["result_count"], 3);
    }

    #[test]
    fn reward_starts_neutral_and_learns() {
        let mut r = Reward::default();
        assert_eq!(r.mean(), 0.5);
        assert_eq!(r.weight(), 0.75);
        for _ in 0..8 {
            r.update(true);
        }
        assert!(
            r.mean() > 0.85,
            "repeated positive feedback raises the mean"
        );
        assert!(r.weight() > Reward::default().weight());
    }
}
