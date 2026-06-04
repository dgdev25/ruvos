# ADR-012: Resource control and worktree orchestration

**Status:** Proposed (2026-06-04)
**Phase:** 3
**Goal:** keep agents fast, isolated, and bounded

## Context

Agentic systems fail in practice when they overrun context, touch too much state,
or collide while editing the same repository. rUvOS needs a resource layer that
can bound work before it becomes expensive or unsafe.

## Decision

Add per-task resource budgets for:

- time,
- tokens,
- tool calls,
- file scope,
- retry count.

Add worktree orchestration so each task can run in an isolated git sandbox and
merge back with provenance when complete.

## Consequences

- **+** Better parallelism with fewer merge collisions.
- **+** Smaller, more predictable agent runs.
- **+** Clear operator control over cost and blast radius.
- **−** Needs disciplined cleanup so sandboxes do not accumulate.

## Validation

- Budget enforcement should be testable at the task graph boundary.
- Worktree lifecycle should be covered by integration tests.
- Merge handoff should preserve provenance and artifact lineage.

