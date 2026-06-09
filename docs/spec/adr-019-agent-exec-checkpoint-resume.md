# ADR-019: agent_exec Checkpoint and Partial-Failure Resume

**Status:** Proposed  
**Date:** 2026-06-09  
**Gap:** #15 in gap-register.md

## Context

`ruvos_agent_exec` accepts a list of ops and executes them sequentially. If op 8 of 12 fails (e.g. a compile error, a flaky test, a network timeout), all 12 ops must be re-run from scratch on the next invocation. For long codegen runs (write 5 files → run tests → git commit → push), re-running from scratch wastes time and risks double-writes.

During Sprint 7 and 8 this was observed directly: Python patcher scripts were re-run multiple times, each time rewriting files that had already been correctly patched.

## Decision

Add a journal to `agent_exec`:

1. Before executing each op, write a checkpoint entry to `~/.ruvos/exec-journal/{journal_id}/{op_index}.json` containing the op spec and status `pending`.
2. On success, update the entry to `completed` with a result digest.
3. On failure, update to `failed` with the error.
4. When `agent_exec` is called with a `resume_journal_id`, skip all ops whose checkpoint is `completed`, replay failed and pending ops only.

The journal ID is returned in the `agent_exec` response. The caller (coordinator) stores it and passes it back on retry.

Journal entries are retained for 24 hours then auto-purged.

## Consequences

**Positive:**
- Retries are fast: only failed/pending ops re-run
- No double-writes of already-completed file ops
- Long pipelines (12+ ops) become resilient to transient failures

**Trade-offs:**
- Journal storage adds I/O overhead per op (mitigated: small JSON writes)
- Resume assumes op idempotency for non-skipped ops — `run_command` ops (e.g. test runs) are inherently idempotent; `write_file` ops must detect existing content to avoid partial overwrites
- Journal ID management adds state the coordinator must carry between calls

## Alternatives Considered

- **No resume, just retry all**: current behaviour. Simple but wasteful. Rejected for large-project use.
- **Transactional rollback on failure**: complex, not needed — partial completion (first 7 ops succeeded) is usually preferable to full rollback.
