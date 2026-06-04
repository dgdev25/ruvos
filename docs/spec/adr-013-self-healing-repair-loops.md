# ADR-013: Self-healing repair loops

**Status:** Proposed (2026-06-04)
**Phase:** 4
**Goal:** turn failure into a structured recovery path

## Context

The current system can stop on failure, but that is only half of an autonomous
runtime. To feel agentic, rUvOS must classify failures, choose recovery
strategies, and continue when the recovery is plausible.

## Decision

Add structured failure classification and repair loops that can:

- detect the kind of failure that occurred,
- choose a recovery plan,
- retry with adapted context,
- learn which repair paths worked.

This phase should lean on the existing GOAP and graph-flow substrate rather than
inventing another workflow engine.

## Consequences

- **+** Work becomes self-correcting instead of merely failing loudly.
- **+** The system can adapt to transient and recoverable errors.
- **+** Repair outcomes become learning signals.
- **−** Recovery policy needs guardrails to avoid infinite loops.

## Validation

- Failure classification should be unit-tested on representative failure cases.
- Recovery loops should prove bounded retries and eventual termination.
- Successful repair should emit a learning signal to the audit/event stream.

