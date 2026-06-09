# ADR-020: Semantic Memory Search — Status of Retrieval Infrastructure

**Status:** Accepted (infrastructure already present; neural embedding deferred)
**Date:** 2026-06-09 (revised after source inspection)
**Gap:** #16 in gap-register.md
**Supersedes:** the original "embed arroy + fastembed-rs" proposal (rejected — see Decision)

## Context

The original ADR-020 asserted that `ruvos_memory_search` and `ruvos_intel_pattern_search`
"use keyword matching" and could not answer semantic-similarity queries. **Source
inspection on 2026-06-09 showed this premise is false.** The retrieval stack is already
real and multi-tiered:

- **`embedding.rs`** — `embed()` produces 384-dim L2-normalised dense vectors; ANN
  retrieval runs on `ruvector-core`'s real **HNSW** index (`hnsw_rank`), a predicate-aware
  **ACORN** filtered-HNSW path (`acorn_rank`) for tag-scoped queries, and an in-memory
  **RaBitQ** (1-bit quantized + exact rerank) path for small candidate sets.
- **`memory.rs` (`memory.search`)** — Tier 1 ANN retrieval (HNSW or ACORN by filter
  presence) → BM25 sparse pass for exact/rare-term recall → **MMR** re-ranking for
  diversity → recency boost → bandit-feedback reweighting (ADR-005). Tier 2 federates
  through RuLake.
- **`intel.rs` (`intel.pattern_search`)** — TF-cosine disk recall **plus** SONA
  **ReasoningBank** in-memory K-means cluster similarity, which finds structurally similar
  trajectories even when keywords differ.

The genuine limitation is one level deeper than the ADR claimed: `embed()` is a
**FNV-1a feature-hashing** embedder ("the hashing trick"). It is deterministic, offline,
and zero-dependency — but it matches on **token overlap**, not meaning. "auth middleware"
and "authentication interceptor" share no tokens and therefore land far apart in vector
space. A learned (neural) embedder would close that semantic gap.

## Decision

**1. Reject the arroy + fastembed-rs stack.** `fastembed-rs` pulls in **ONNX Runtime
(`ort`)**, a heavy C++ dependency. Adding it would:
- break the workspace's `unsafe_code = "forbid"` / pure-Rust posture (C++ FFI);
- reintroduce a multi-minute cold-compile — the exact problem just eliminated by removing
  `reqwest`/`ring`/`rustls` (see ADR-032 history);
- add a ~23 MB first-run model download and an out-of-process model cache to manage.

`arroy` (LMDB ANN) is also redundant: `ruvector-core` HNSW + ACORN already provide
embeddable, in-process ANN with no new storage engine.

**2. Record that the retrieval infrastructure satisfies the gap's intent.** Semantic-style
retrieval (ANN + MMR + cluster similarity + sparse fusion) is implemented and shipping.
Gap #16's retrieval requirement is met.

**3. Defer neural embedding as scoped future work.** When semantic recall across
divergent vocabulary becomes a measured bottleneck, the upgrade is isolated to `embed()` —
every downstream consumer (HNSW/ACORN/RaBitQ/MMR) is embedder-agnostic and needs no change.
The constraint-respecting implementation path (NOT done now) is:

- Route embedding through **OpenRouter** (the only permitted API key) via a **curl
  subprocess** — the same pattern as the ADR-032 `CliRouter`. No new crates, no ONNX.
- Add a content-addressed **on-disk embedding cache** (`~/.ruvos/embed-cache/<hash>.vec`)
  and a **batch** embed call, so HNSW indexing loops that call `embed()` per item do not
  issue N network requests.
- Keep the FNV-1a feature-hash embedder as the **offline default** when
  `OPENROUTER_API_KEY` is unset — preserving determinism and zero-dependency operation.
- `EMBED_DIM` stays 384 so the existing indices need no reshape if a 384-dim model is used;
  a dimension change requires an index rebuild (migration note for that future ADR).

## Consequences

**Positive:**
- No heavy C++/ONNX dependency; pure-Rust guarantee and fast compile preserved.
- The shipping retrieval stack is documented accurately; the gap register reflects reality.
- The future neural-embedding upgrade is a single-function change behind a stable interface.

**Trade-offs:**
- Until the deferred work lands, cross-vocabulary semantic matches rely on token overlap
  plus SONA cluster similarity — weaker than a learned embedder for paraphrase-heavy queries.
- The eventual OpenRouter embedding path adds network latency on cache miss (mitigated by
  the on-disk cache) and depends on `OPENROUTER_API_KEY` for its best-quality mode.

## Alternatives Considered

- **fastembed-rs + arroy (original proposal):** rejected — ONNX C++ dep, compile-time
  regression, redundant ANN engine. See Decision.
- **usearch / hora:** C++ core or uncertain maintenance; also redundant given ruvector-core.
- **External embedding API (OpenAI/Anthropic direct):** violates the no-Anthropic-key and
  CLI-first constraints. OpenRouter-via-curl is the only permitted networked path.
- **Status quo forever:** acceptable at current scale; revisit when semantic recall is a
  measured bottleneck.
