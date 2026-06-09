# ADR-021: Parallel Ops in agent_exec via parallel_group

**Status:** Proposed  
**Date:** 2026-06-09  
**Gap:** #17 in gap-register.md

## Context

All ops in a single `ruvos_agent_exec` call execute sequentially. Writing three independent widget files in Sprint 8 took three sequential write_file ops when they could run concurrently. As sprint scope grows (Sprint 10+ will involve 8-10 independent file writes), sequential execution wastes wall-clock time.

## Decision

Add a `parallel_group` op that wraps a list of child ops and executes them concurrently using `tokio::spawn`:

```json
{"op": "parallel_group", "ops": [
  {"op": "write_file", "path": "a.ts", "content": "..."},
  {"op": "write_file", "path": "b.ts", "content": "..."},
  {"op": "run_command", "cmd": "...", "args": [...]}
]}
```

All child ops start simultaneously. The `parallel_group` op completes when all children complete. If any child fails, the group is marked failed and the error from the first failure is reported; other children are awaited to completion (not cancelled) to avoid partial file writes.

Nested `parallel_group` ops are not permitted (depth limit of 1).

## Consequences

**Positive:**
- Independent file writes complete in parallel — wall-clock time scales with the slowest single op, not the sum
- Run_command ops (e.g. `cargo clippy` and `vitest run` in separate directories) can fire simultaneously

**Trade-offs:**
- Race conditions if two ops write to the same file — caller's responsibility to avoid; ruvos does not lock files
- Journal checkpointing (ADR-019) must handle parallel ops: each child op gets its own checkpoint entry; the group entry is `completed` only when all children are `completed`
- Error reporting is more complex: partial success within a group must be reported per-child

## Alternatives Considered

- **`parallel: true` flag per op**: requires scanning the entire op list to group adjacent parallel ops. Less explicit. Rejected.
- **Multiple concurrent `agent_exec` calls**: possible today but requires the caller to coordinate; no shared journal, no error aggregation. Rejected for pipelines.
