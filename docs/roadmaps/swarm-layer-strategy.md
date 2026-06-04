# Swarm Layer Strategy

This document defines the first-class swarm layer to rebuild on top of rUvOS.
The goal is not to add another execution engine. The goal is to add a thin
control plane that coordinates many agents through explicit, durable state.
At the MCP boundary these are exposed as tools, but at the swarm-design level
they are control-plane commands.

## What we already have in rUvOS

- `agent.spawn` for real worker execution
- `orchestrate.run` for plan → execute → validate pipelines
- `relay.*` for cross-instance discovery and message transport
- `memory.*` and `intel.*` for durable context and learned intent
- `gov.events`, `gov.replay`, and `gov.report` for audit, replay, and metrics

## Swarm design

The swarm layer should be a coordinator, not a competing runtime.

### Core principles

- Hierarchical control by default.
- Mesh communication as an overlay, not the primary control model.
- Durable state for membership, ownership, leases, and task assignment.
- Event-sourced changes so replay and reporting are always possible.
- Workers do work; the swarm coordinates work.

### Recommended topology

- `hierarchical` as the default
- `mesh` as an optional peer overlay
- `hybrid` for most real tasks
- `adaptive` later, once metrics justify it

## First command surface

The first version of `swarm.*` should expose these commands:

- `swarm.create` — create a swarm; topology is inferred from the task unless explicitly provided
- `swarm.status` — inspect the active swarm, members, and current progress
- `swarm.assign` — assign a task to a named swarm member and persist the handoff
- `swarm.heartbeat` — refresh a member's liveness and lease state
- `swarm.message` — send a direct or broadcast message between members

Implemented in the initial control-plane slice:

- `swarm.complete` — mark the swarm finished and persist the final summary
- `swarm.fail` — mark the swarm failed with a recorded reason
- `swarm.health` — report member freshness, utilization, and liveness
- `swarm.rebalance` — move tasks off stale members onto live members
- `swarm.join` — add or reactivate a swarm member
- `swarm.leave` — mark a swarm member as left
- `swarm.report` — generate a swarm summary with recent activity
- `swarm.metrics` — return numeric swarm health and throughput metrics

## Data model

The swarm state should track:

- swarm id
- objective
- topology
- coordinator
- members
- roles
- leases / heartbeats
- assignment queue
- status
- timestamps

## Implementation mapping in rUvOS

- Persistent state: `crates/ruvos-mcp/src/paths.rs`
- Swarm store: `crates/ruvos-mcp/src/swarm.rs`
- MCP tool handlers: `crates/ruvos-mcp/src/tools/swarm.rs`
- Registry wiring: `crates/ruvos-mcp/src/tools/mod.rs`
- Audit/replay/reporting: `crates/ruvos-mcp/src/tools/gov.rs`
- Transport and coordination: `crates/ruvos-mcp/src/relay.rs`

## Rollout plan

1. Add durable swarm state and `swarm.create` / `swarm.status`.
2. Add assignment and heartbeat management.
3. Add swarm messaging.
4. Add swarm completion and failure lifecycle commands.
5. Add swarm health reporting.
6. Add swarm membership lifecycle commands.
7. Add swarm metrics and reporting to governance reports.
