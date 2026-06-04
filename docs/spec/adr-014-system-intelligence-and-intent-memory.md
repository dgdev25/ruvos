# ADR-014: System intelligence and intent memory

**Status:** Proposed (2026-06-04)
**Phase:** 5
**Goal:** let the OS remember what matters structurally, not just factually

## Context

rUvOS has semantic memory and knowledge graph pieces, but the OS still lacks a
system-level model of the repo and the user's recurring intent. It can recall
facts, but it does not yet reason about ownership, hotspots, or stable goals as
first-class runtime objects.

## Decision

Add two intelligence layers:

1. **Repo intelligence** - architecture maps, ownership graphs, hot paths, test
   gaps, and dependency risk.
2. **Intent memory** - stable goals, preferences, and recurring workflows that
   influence future planning and routing.

Add periodic synthesis jobs that turn repeated actions into durable docs,
templates, or policies.

## Consequences

- **+** The system gets better at predicting what the user wants next.
- **+** The repo becomes navigable as a living graph rather than a file tree.
- **+** Repeated decisions can be consolidated into usable defaults.
- **−** Needs strong provenance so synthesized knowledge stays trustworthy.

## Validation

- Intelligence outputs should be reproducible from stored events and artifacts.
- Intent-memory updates should be observable and queryable.
- Synthesized docs and policies should be traceable back to source signals.

