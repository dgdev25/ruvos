# ADR-015: Coordination layer and collaboration contracts

**Status:** Proposed (2026-06-04)
**Phase:** 6
**Goal:** make multi-agent work explicit rather than incidental

## Context

`relay` currently moves messages between instances, but agentic coordination
needs more than delivery. Agents need ownership, handoff rules, and conflict
resolution so a swarm can cooperate without stepping on itself.

## Decision

Add coordination contracts for:

- role ownership,
- handoff rules,
- conflict resolution,
- system-state visibility for sessions, agents, goals, blockers, and health.

Evolve relay from a mailbox into a structured collaboration layer while keeping
the file-based, daemon-free transport model.

## Consequences

- **+** Multi-agent work becomes auditable and explicit.
- **+** Humans can see who owns what and why.
- **+** Existing relay mechanics stay useful as the transport layer.
- **−** Adds another layer of policy to keep consistent with autonomy modes.

## Validation

- Role and handoff rules should be covered by integration tests.
- System-state views should match the persisted store.
- Relay messages should remain durable and queryable.

