# ADR-018: Real Multi-Step Orchestrate Pipeline Driver

**Status:** Proposed  
**Date:** 2026-06-09  
**Gap:** #14 in gap-register.md

## Context

`ruvos_orchestrate_run` currently takes a template name (feature/bugfix/refactor/security/sparc) and a task description, and returns a list of `coordinator_steps` — a plan. It does not execute anything. The host (Claude Code) must read the plan and manually invoke each step.

For a large project build, every sprint involves the same manual loop: read orchestrate output → spawn coder → wait → spawn reviewer with coder output → wait → spawn tester → wait → report. This is error-prone and slow.

## Decision

Extend `ruvos_orchestrate_run` with an optional `execute: true` parameter that activates a real pipeline driver:

1. For each step in the template, spawn the appropriate archetype agent via `agent_spawn`
2. Collect each agent's artifact (using swarm scratch slots per ADR-017)
3. Pass the previous step's artifact as context to the next step's prompt
4. Return a `pipeline_result` with each step's outcome, artifacts, and pass/fail status

The driver runs synchronously (waits for each step) in the initial implementation. The `execute: false` default preserves the existing plan-only behaviour.

When `ANTHROPIC_API_KEY` is absent, `execute: true` degrades gracefully to plan-only with a warning.

## Consequences

**Positive:**
- One tool call drives an entire coder→reviewer→tester pipeline without manual coordination
- Closes the gap that makes every sprint require the same boilerplate orchestration by Claude Code
- Template definitions become executable, not just advisory

**Trade-offs:**
- Long-running: a 3-step pipeline could take minutes. Timeout and cancellation handling required.
- Requires ADR-017 (scratch slots) for artifact handoff between steps
- Error in step 2 must not silently skip step 3 — explicit fail-fast vs. continue-on-error option needed

## Alternatives Considered

- **Keep plan-only, host drives manually**: current approach. Scales poorly; every sprint requires identical boilerplate. Rejected.
- **Daemon-based async pipeline**: have `ruvos daemon watch` drive the pipeline asynchronously (relay messages). More powerful but more complex. Deferred to ADR for daemon layer.
