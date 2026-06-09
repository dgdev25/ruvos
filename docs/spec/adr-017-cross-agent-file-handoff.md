# ADR-017: Cross-Agent File Handoff via Swarm Scratch Slots

**Status:** Proposed  
**Date:** 2026-06-09  
**Gap:** #10 in gap-register.md

## Context

Agents spawned via `ruvos_agent_exec` can write files independently, but there is no mechanism for agent A to hand an artifact to agent B within a pipeline. The current workaround is storing file content in `ruvos_memory_store` and retrieving it with `ruvos_memory_retrieve`, which is lossy (truncation risk on large files), untyped, and requires the consumer to know the exact memory key.

A coder agent that writes `src/lib.rs` has no way to signal to a reviewer agent "here is the file I just wrote" without the coordinator knowing and hard-coding the path. This makes orchestrate pipelines fragile.

## Decision

Add two new ops to `ruvos_agent_exec`:

- **`write_slot`**: writes a named artifact (path + content) to the current swarm's scratch space (`~/.ruvos/swarms/{swarm_id}/slots/{slot_name}`). Optionally tags with the source agent ID and timestamp.
- **`read_slot`**: reads a named slot from the current swarm scratch space, returning path + content. Blocks until the slot exists (with configurable timeout) enabling producer/consumer synchronisation.

The scratch space is scoped to a swarm ID, is cleared on `swarm_complete`, and is not part of the persistent memory store — it is ephemeral pipeline glue, not long-term memory.

## Consequences

**Positive:**
- Closes Gap 10 cleanly without coupling agents to specific file paths
- Enables `coder → reviewer → tester` pipelines where each stage reads the previous stage's output by slot name
- Scratch space is isolated per swarm; no cross-swarm data leakage

**Trade-offs:**
- Adds two new op types to `agent_exec` (tool schema update required; contract manifest regeneration needed)
- `read_slot` blocking semantics require a timeout to avoid deadlock — must be tunable
- Ephemeral by design: slots disappear on swarm completion, so operators cannot inspect post-mortem without explicit archiving

## Alternatives Considered

- **Memory bridge** (current workaround): lossy, untyped, requires shared key knowledge. Rejected for production pipelines.
- **Shared filesystem path convention**: agents agree on `/tmp/ruvos-{swarm_id}/` paths. Works but requires coordinator to hard-code all paths. Rejected as fragile.
- **Relay bus message with file content**: use `relay_send` with file bytes in body. Rejected — relay is for coordination signals, not bulk data transfer.
