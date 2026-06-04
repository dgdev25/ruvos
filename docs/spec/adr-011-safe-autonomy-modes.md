# ADR-011: Safe autonomy modes and durable jobs

**Status:** Proposed (2026-06-04)
**Phase:** 2
**Goal:** let the system act unattended without becoming unpredictable

## Context

rUvOS can already spawn agents, but it does not yet distinguish between:

- a user that wants advice,
- a user that wants delegated execution,
- a user that wants the system to continue without intervention.

It also lacks a first-class durable job model with pause/resume/checkpoint
semantics. That makes long-running work fragile and encourages brittle ad hoc
loops.

## Decision

Add explicit autonomy modes:

- `manual` - never act without confirmation.
- `assist` - suggest actions, but ask before execution.
- `delegate` - execute within policy boundaries and report checkpoints.
- `autopilot` - continue until completion or policy stop conditions.

Add durable jobs with persisted lifecycle state, checkpoints, and restart
recovery so long-running work survives process exits.

## Consequences

- **+** The system can be trusted with unattended work when appropriate.
- **+** The user gets predictable escalation behavior.
- **+** Jobs become resumable instead of ephemeral.
- **−** Requires clear policy defaults to avoid overreach.

## Validation

- Autonomy mode transitions should be covered by unit tests.
- Durable jobs should survive restart in integration tests.
- Strict mode should fail fast when a job exceeds policy or budget.

