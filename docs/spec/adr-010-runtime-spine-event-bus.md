# ADR-010: Runtime spine - event bus, task graph, and policy boundary

**Status:** Proposed (2026-06-04)
**Phase:** 1
**Goal:** make rUvOS event-driven instead of command-driven

## Context

rUvOS already persists actions and audit events, but the runtime does not yet
have a single model for "what happened", "what is blocked", or "what should
happen next". Tool handlers independently do their work and write state, but
there is no shared execution spine that can:

- fan out events to learning, safety, and scheduling subsystems;
- represent a workflow as a task graph rather than a linear command chain;
- enforce a uniform policy boundary across tools, files, network, and
  destructive operations.

Without this spine, autonomy stays fragmented: the system can execute tools, but
it cannot yet reason about itself as a running operating system.

## Decision

Introduce a runtime spine with three primitives:

1. **Event bus** - a structured event stream for tool calls, agent actions,
   hook transitions, retries, artifacts, and policy decisions.
2. **Task graph** - a durable dependency graph for sessions, agents, relay
   messages, and follow-up work, with explicit ready/running/blocked/completed
   states.
3. **Policy boundary** - a single permission vocabulary for tool scopes, file
   scopes, network scopes, and destructive operations.

The first implementation should stay pure Rust and reuse the existing store and
event records rather than introduce a second persistence system.

## Consequences

- **+** Every subsystem can subscribe to one runtime stream instead of inventing
  its own state conventions.
- **+** Workflows become schedulable, replayable, and inspectable.
- **+** Policy decisions become explicit runtime data rather than scattered
  checks.
- **−** Adds a new core abstraction that must stay small and stable.

## Validation

- `crates/ruvos-mcp/src/runtime.rs` should cover the event envelope and task-graph primitives.
- `gov.events` should be able to query runtime events through the shared store.
- Integration coverage should prove that tool calls still produce persisted events.

