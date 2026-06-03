# ADR-005: Hybrid (BM25 + dense) retrieval and a bandit feedback loop for `memory`

**Status:** Accepted (2026-06-03)
**Amends:** scope-ledger-v1.md §1 (`memory.search`, `memory.retrieve`)
**Tier:** 1 · **Source:** rUvnet `agentdb` (algorithms; **rebuilt in Rust**, not imported)

## Context

`memory.search` ranks with **dense** vectors only (feature-hashed embeddings →
HNSW/RaBitQ/ACORN → MMR → recency). Two well-established improvements are missing:

1. **Hybrid retrieval.** Dense vectors miss exact-term/rare-token matches (IDs,
   error codes, API names). SOTA retrieval fuses **sparse lexical (BM25)** with
   dense and is strictly better than either alone, especially for short
   keyword-ish queries.

2. **No learning from use.** Search is static: it never learns which results were
   actually useful. rUvnet's **agentdb** wraps the RuVector engine with a
   **bandit** (Thompson-sampling) loop that reweights retrieval by observed
   reward. agentdb itself is **TypeScript over a NAPI binding** — so we rebuild
   the *algorithms* natively in Rust (the vector engine is already our substrate).

## Decision

1. **Add a BM25 sparse scorer** (`crates/ruvos-mcp/src/tools/retrieval.rs`, pure
   Rust, reusing the existing `tokenize`): per-namespace term frequencies + IDF,
   Okapi BM25 (`k1=1.2, b=0.75`). Computed over the candidate set at query time
   (no separate persistent index needed at current scale).

2. **Fuse sparse + dense via Reciprocal Rank Fusion (RRF)** before MMR:
   `score(d) = Σ 1/(k + rank_i(d))` over the dense and BM25 rankings (`k=60`).
   RRF is scale-free (no score normalization headaches) and robust. MMR diversity
   and the recency blend run **on the fused list**, unchanged downstream.

3. **Add a bandit reward signal.** Persist a per-`(namespace, key)` Beta(α,β)
   reward record in the store. A new optional `memory.search` arg
   `feedback:[{key, useful:bool}]` (and an implicit signal when `memory.retrieve`
   fetches a key shortly after a search) updates the record (`useful`→α+1,
   else β+1). Ranking multiplies the fused score by the posterior mean
   `α/(α+β)` (Thompson-sampled at low traffic), so proven-useful entries rise.
   Cold entries default to a neutral prior (α=β=1) — **no regression** before any
   feedback exists.

4. **Defaults preserve today's behavior.** With no feedback recorded and BM25
   contributing alongside dense, results for existing callers stay stable or
   improve; the bandit term is a tie-breaker, not a dominator (capped weight).
   No new tool — `memory.search`/`memory.retrieve` signatures gain optional args
   only, so the 24-tool count and MCP surface are unchanged.

## Consequences

- **+** Materially better recall on keyword/rare-token queries (hybrid) and a
  self-improving ranking that dovetails with `sona`/`intel` (the flywheel).
- **+** All pure Rust; reuses existing embeddings, store, and `tokenize`.
- **−** Adds a reward table to the store and a feedback path the caller (or an
  implicit heuristic) must drive to realize the learning benefit; hybrid adds a
  modest per-query BM25 pass over the candidate set.
- **Zero-defect:** new `retrieval.rs` ≤500 LOC; unit tests for BM25 correctness,
  RRF fusion ordering, and bandit convergence; cross-process persistence test for
  the reward table.

## Alternatives considered

- **Import agentdb** — impossible cleanly: it's TS/NAPI; we already own the Rust
  vector engine, so we rebuild only the scorer + bandit (small, well-understood).
- **Dense-only + more tuning** — rejected: cannot recover exact-term matches BM25
  gives for free.
- **A heavy learning-to-rank model** — rejected: violates "no local inference in
  v1"; a Beta-Bernoulli bandit is online, tiny, and explainable.

## Rollout

Implementation plan: `docs/superpowers/plans/2026-06-03-hybrid-retrieval-memory.md`.
Phasing: (1) BM25 scorer; (2) RRF fusion into `memory.search`; (3) bandit reward
table + feedback arg + ranking term; (4) implicit retrieve-after-search signal.
Each phase is independently shippable and default-safe.
