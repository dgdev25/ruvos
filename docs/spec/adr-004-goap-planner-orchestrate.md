# ADR-004: GOAP planner for `orchestrate` (computed pipelines, not scripts)

**Status:** Implemented (2026-06-03)
**Amends:** scope-ledger-v1.md §1 (`orchestrate.run`); adds substrate crate `ruvos-goap`
**Tier:** 1 (flagship) · **Source:** rUvnet `ARCADIA` (`src/ai/goap.rs`)

## Context

`orchestrate.run` today maps a `template` string to a **hardcoded** archetype
sequence (`feature` → planner→coder→tester→reviewer, etc.). That is a scripted
pipeline, not planning: it cannot adapt the sequence to the task, skip steps that
are already satisfied, insert steps a goal requires, or compose a pipeline for a
goal that has no template. For an "agentic OS," the orchestrator computing its own
plan is the single highest-value capability gap.

rUvnet's **ARCADIA** contains a self-contained, pure-Rust **GOAP** (Goal-Oriented
Action Planning) engine in `src/ai/goap.rs` (544 LOC): `WorldState`,
`GoapAction { preconditions, effects, cost }`, `GoapGoal { desired_state }`, and
`GoapPlanner::plan()` which runs **A\*** over the action space to find a
minimum-cost action sequence reaching the goal. The module depends only on `std`
+ `serde` — ARCADIA's heavy deps (axum, sqlx, qdrant, reqwest, wasm) live in
*other* modules and are **not** pulled in.

## Decision

1. **Extract, don't depend.** Vendor `goap.rs` into a new pure-Rust substrate
   crate **`substrate/ruvos-goap`** (clean-room, attributed to rUv/ARCADIA, MIT).
   The 544-LOC file is split into `types.rs` (state/action/goal) + `planner.rs`
   (A\* engine) to honor the ≤500-line CI rule. No new external dependencies.

2. **Model orchestration as a planning problem.** Each agent archetype becomes a
   `GoapAction` with declared `preconditions`/`effects` over a small feature-dev
   `WorldState` (e.g. `coder` requires `plan_exists=true`, produces `code=true`;
   `tester` requires `code=true`, produces `tested=true`). A `template` becomes a
   `GoapGoal` (desired end-state, e.g. `reviewed=true ∧ tested=true`). The planner
   **derives** the archetype order via A\*.

3. **`orchestrate.run` runs the plan, with a safe fallback.** The handler builds
   the action library + goal, calls the planner, and executes the resulting
   sequence (unchanged execution + artifact mechanics). If planning returns no
   plan (mis-specified goal), it **falls back to the existing static template** so
   behavior never regresses. Response gains `planned: bool`, `plan_cost: f64`; the
   `steps[]` shape is unchanged.

4. **Optional caller-driven goals.** `orchestrate.run` accepts optional
   `goal` (desired world-state) and `capabilities` (extra actions) so a caller can
   request a *computed* pipeline beyond the four named templates. `template`
   remains fully supported (it just seeds the goal), so this is **backward
   compatible** — no MCP surface/tool count change.

## Consequences

- **+** `orchestrate` becomes adaptive: skips satisfied steps, composes novel
  pipelines, optimizes by cost. Pure Rust, single binary preserved.
- **+** A reusable planning primitive (`ruvos-goap`) other domains can use later
  (e.g. `hooks.route` multi-step routing).
- **−** New substrate crate (+~550 LOC vendored) and added modeling surface (the
  archetype precondition/effect library must be maintained). Mitigated by the
  fallback and by keeping the action library small and data-driven.
- **Zero-defect:** `ruvos-goap` joins the workspace as a member (built, clippy,
  fmt, tested in CI). ARCADIA's own GOAP unit tests are ported.

## Validation

- Covered by `substrate/ruvos-goap/src/lib.rs` tests.
- Covered by `crates/ruvos-mcp/src/tools/orchestrate_plan.rs` planning tests.
- Exercised by `crates/ruvos-mcp/src/tools/orchestrate.rs` and the MCP integration test.

## Alternatives considered

- **Depend on the ARCADIA crate** — rejected: pulls axum/sqlx/qdrant/reqwest/wasm,
  violating the single-binary / no-bloat policy.
- **Hand-roll a planner** — rejected: ARCADIA's A\* GOAP is correct, tested, and
  rUvnet-original; rebuilding it from scratch wastes the IP.
- **Keep static templates** — rejected: that is the gap this ADR closes.

## Rollout

Implementation plan: `docs/superpowers/plans/2026-06-03-goap-orchestrate.md`.
Phasing: (1) `ruvos-goap` crate + ported tests; (2) archetype action library +
goal builder; (3) wire handler with fallback; (4) optional `goal`/`capabilities`
args. Complements ADR-007 (graph-flow executes branching plans) and ADR-006
(SPARC as a goal/template) — adopt GOAP first.
