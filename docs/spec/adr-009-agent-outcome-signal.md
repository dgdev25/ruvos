# ADR-009: A real per-step outcome signal (success / failure) for agents and `orchestrate`

**Status:** Implemented (2026-06-03)
**Amends:** scope-ledger-v1.md §1 (`agent.spawn`, `orchestrate.run` responses)
**Unlocks:** ADR-007 (graph-flow branching), meaningful `hooks.post`/`intel` learning, the bandit flywheel
**Tier:** keystone prerequisite (identified while deferring ADR-007)

## Context

Every `agent.spawn` returns `status: "completed"` unconditionally, and the
optional external runner (`RUVOS_AGENT_RUNNER`) **captures the subprocess output
but discards its exit status**. So rUvOS has no notion of whether a step
*succeeded* or *failed*. That single absence blocks several things:

- **graph-flow (ADR-007)** has no condition to branch on (deferred for this reason).
- **`hooks.post` / `intel` / SONA** record outcomes, but the outcome is always
  "success" — there is nothing real to learn from.
- **`orchestrate`** runs every step regardless; a failed `tester` still proceeds
  to `reviewer`.

A real signal already exists and is being thrown away: the runner's **process
exit code**.

## Decision

1. **Agents return a structured outcome.** `agent.spawn`'s task execution yields
   `success: bool` and `exit_code: Option<i32>`:
   - **Runner configured:** `success = exit_status.success()`,
     `exit_code = exit_status.code()`; on failure, stderr is appended to the
     result text. This is a **real** signal (process exit code).
   - **No runner:** `success = true`, `exit_code = null` — honestly labeled
     "no executor; artifact produced" (unchanged default behavior).
   The response `status` becomes `"completed"` (success) or `"failed"`, and gains
   `success` + `exit_code`. The persisted agent record + `gov.events` carry the
   real status.

2. **`orchestrate` threads per-step outcome and stops on failure.** Each step
   records `success`; **on the first failed step the pipeline stops** (a failed
   `tester` does not run `reviewer`). The response gains a top-level `success`
   and per-step `success`; overall `status` is `"failed"` if any step failed.
   This is the first genuine *branch* in orchestration — enabled by the signal,
   without yet needing graph-flow.

3. **Backward compatible.** With no runner (the default, and all current tests),
   every step succeeds → `status:"completed"` exactly as before. The new fields
   are additive.

## Consequences

- **+** Real "did it work?" signal across agents + orchestration; failed
  pipelines stop early instead of wasting steps.
- **+** Unlocks ADR-007 (conditional edges have a real condition) and gives
  `hooks.post`/`intel`/the bandit something true to learn from.
- **+** Pure Rust, no new deps — just stop discarding `output.status`.
- **−** Behavior change when a runner *is* configured and fails (previously
  silently "completed"); this is the point. Default path unchanged.
- **Zero-defect:** unit tests for success/failure outcome (runner exit 0 vs ≠0,
  `#[cfg(unix)]` for the failing-runner case), orchestrate stop-on-failure, and
  the unchanged no-runner default.

## Alternatives considered

- **Synthetic/heuristic success** (e.g. "artifact non-empty") — rejected: fake
  signal; worse than honest "assumed success" when no executor runs.
- **Require agents to truly run tests** — out of scope; the exit code of whatever
  the runner wraps is the real, available signal today.
- **Leave it** — rejected: it's the keystone blocking ADR-007 and real learning.

## Rollout

Implemented directly (small, contained). After this lands, ADR-007 can be
revisited: a plan that needs a retry/branch on a failed step now has a real
condition to test.
