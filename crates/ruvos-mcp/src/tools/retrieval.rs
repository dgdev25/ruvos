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
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const K1: f32 = 1.2;
const B: f32 = 0.75;
const RRF_K: f32 = 60.0;

/// Rank `docs` (id, text) against `query` by Okapi BM25; returns up to `k` ids,
/// best-first, dropping zero-score docs. Computed over the candidate set — no
/// persistent index needed at current scale.
pub fn bm25_rank(docs: &[(String, String)], query: &str, k: usize) -> Vec<String> {
    if docs.is_empty() || k == 0 {
        return Vec::new();
    }
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
                    idf * (f * (K1 + 1.0)) / (f + K1 * (1.0 - B + B * dl / avgdl))
                })
                .sum();
            (i, score)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored
        .into_iter()
        .filter(|(_, s)| *s > 0.0)
        .take(k)
        .map(|(i, _)| docs[i].0.clone())
        .collect()
}

/// Reciprocal Rank Fusion of several rankings (each best-first). Scale-free:
/// `score(d) = Σ 1/(RRF_K + rank_i(d))`. Returns up to `k` ids, best-first.
pub fn rrf_fuse(rankings: &[&[String]], k: usize) -> Vec<String> {
    let mut score: HashMap<&str, f32> = HashMap::new();
    for ranking in rankings {
        for (rank, id) in ranking.iter().enumerate() {
            *score.entry(id.as_str()).or_insert(0.0) += 1.0 / (RRF_K + rank as f32 + 1.0);
        }
    }
    let mut v: Vec<(&str, f32)> = score.into_iter().collect();
    // Stable, deterministic: by score desc, then id asc to break ties.
    v.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(b.0))
    });
    v.into_iter()
        .take(k)
        .map(|(id, _)| id.to_string())
        .collect()
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
