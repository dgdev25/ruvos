# ADR-020: Semantic Memory Search via arroy + fastembed-rs

**Status:** Proposed  
**Date:** 2026-06-09  
**Gap:** #16 in gap-register.md  
**Source repos:** meilisearch/arroy (MIT), Anush008/fastembed-rs (Apache-2.0)

## Context

`ruvos_memory_search` and `ruvos_intel_pattern_search` use keyword matching. For a large codebase, "find patterns similar to this auth middleware" or "retrieve memories related to the ForgeCMS widget system" cannot be answered by substring match — they require semantic (vector) similarity.

The stress-test session revealed this concretely: searching for prior sprint patterns returned nothing useful because widget implementations from Sprint 3 use different vocabulary than Sprint 8, even though they share structural patterns.

## Decision

Embed a two-crate stack into `ruvos-store`:

1. **arroy** (MIT, meilisearch) — LMDB-backed approximate nearest neighbours with random-projection indexing. Pure Rust, embeddable, no separate process. Index is stored alongside the existing redb store.
2. **fastembed-rs** (Apache-2.0, compatible with ruvos MIT distribution) — local ONNX embedding generation. Default model: `all-MiniLM-L6-v2` (384 dims, ~23MB). No GPU, no external API call required.

Behaviour change:
- `memory_store` and `intel_pattern_store` embed the content/trajectory at write time and insert into the arroy index alongside the redb record.
- `memory_search` and `intel_pattern_search` accept a new optional `semantic: true` flag. When set, the query is embedded and the top-K nearest neighbours are retrieved from arroy, then reranked by redb metadata filters.
- Keyword search remains the default to preserve backwards compatibility.

## Consequences

**Positive:**
- "Find patterns similar to X" works across vocabulary differences
- Fully local — no embedding API key, no latency spike from external calls
- arroy's LMDB backing means the vector index and the KV store share one process boundary

**Trade-offs:**
- fastembed-rs is Apache-2.0 (not MIT); acceptable for distribution but must be documented in NOTICE file
- First-run embedding model download (~23MB); subsequent runs use local cache
- arroy index must be rebuilt if the embedding model changes (migration tooling needed)
- `semantic: true` adds ~5-50ms latency per search vs. keyword scan

## Alternatives Considered

- **usearch** (Apache-2.0, 4,200 stars, C++ core): higher performance but C dependency breaks pure-Rust guarantee. Rejected.
- **hora-search/hora** (Apache-2.0): maintenance status uncertain. Rejected.
- **External embedding API** (OpenAI/Anthropic): adds network dependency, API key, latency, cost. Rejected.
- **cogniplex/codemem patterns** (Apache-2.0): graph-vector hybrid; overkill for initial implementation but referenced for future graph-memory ADR.
