# ADR-016: Evaluation and governance

**Status:** Proposed (2026-06-04)
**Phase:** 7
**Goal:** make rUvOS measurable and reconstructable

## Context

An agentic operating system is only trustworthy if it can explain what it did,
why it did it, and whether the result was good. The current system has audit
logs and signatures, but it lacks a formal evaluation and governance layer that
turns traces into operator-visible quality signals.

## Decision

Add:

- replayable session traces from events and artifacts,
- workflow benchmarks for success rate, time-to-completion, intervention rate,
  wasted context, and rollback rate,
- policy audits and governance reports for operators.

## Consequences

- **+** The OS becomes inspectable instead of magical.
- **+** Quality can be tracked over time instead of guessed.
- **+** Governance data becomes a first-class product surface.
- **−** Evaluation needs stable definitions so metrics do not drift.

## Validation

- Replay should reconstruct a session from persisted artifacts and events.
- Benchmarks should be repeatable across runs.
- Governance reports should be derived from the same persisted state as `doctor`.

