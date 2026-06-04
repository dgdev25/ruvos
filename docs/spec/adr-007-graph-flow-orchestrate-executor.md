# ADR-007: `graph-flow` typed DAG executor for `orchestrate` (branching pipelines)

**Status:** Implemented (2026-06-03) — as a lean clean-room executor (see "Implementation note"), opt-in via `max_retries`
**Amends:** scope-ledger-v1.md §1 (`orchestrate.run` execution model); adds substrate crate `ruvos-graphflow`
**Tier:** 2 · **Source:** rUvnet `rs-graph-llm` (`graph-flow` crate)

## Context

`orchestrate.run` executes its pipeline as a **linear `for` loop** over archetypes.
That cannot branch ("if tests fail → loop back to coder"), fan out, or stop early
on a condition. ADR-004 lets the orchestrator *compute* a plan; a richer plan
needs a richer *executor*.

rUvnet's **rs-graph-llm** contains **`graph-flow`** — a small, pure-Rust **typed
DAG workflow executor** (concurrent task graph via `DashMap`, conditional edges,
per-task status). Its sibling crates pull PostgreSQL + the Rig LLM client, but the
**graph/runner core is independent** of those.

## Decision

1. **Vendor only the executor core** into a new pure-Rust substrate crate
   **`substrate/ruvos-graphflow`** (clean-room, attributed, MIT): the node/edge
   types, the conditional-edge model, and the concurrent runner. **Drop**
   PostgreSQL session storage (rUvOS uses `ruvos-store`/redb) and the Rig LLM
   backend (rUvOS executes steps via `agent.spawn`). Keep deps to `dashmap` +
   `serde` (both already in the tree).

2. **`orchestrate` executes a graph, not a list.** A plan (from a template or, per
   ADR-004, from the GOAP planner) is compiled into a `graph-flow` graph where
   each node runs an `agent.spawn` and edges carry **conditions** evaluated on the
   step result (e.g. `tester.status == failed → coder`). Linear templates compile
   to a straight-line graph, so **existing behavior is a special case** — no
   regression.

3. **Bounded loops.** Conditional back-edges (retry loops) are capped by a
   `max_steps`/`max_revisits` guard surfaced in the response (`steps[]`,
   `step_count` unchanged in shape; add `graph: {nodes, edges}` metadata). No new
   tool.

## Consequences

- **+** `orchestrate` supports branch/retry/early-exit — real workflows, not just
  straight lines. Natural execution target for GOAP plans (ADR-004).
- **+** Pure Rust, minimal deps; PostgreSQL/Rig explicitly excluded.
- **−** A second new substrate crate and a plan→graph compile step; only worth
  doing **after** GOAP (ADR-004) creates plans worth branching on. Adopt **one at
  a time** — GOAP first, graph-flow second.
- **Zero-defect:** `ruvos-graphflow` is a workspace member; port graph-flow's core
  tests; add a conditional-retry orchestration test.

## Validation

- Covered by `substrate/ruvos-graphflow/src/lib.rs` tests for branching and bounded failure.
- Exercised by `crates/ruvos-mcp/src/tools/orchestrate.rs` when `max_retries > 0`.

## Alternatives considered

- **Depend on `rs-graph-llm` whole** — rejected: PostgreSQL + Rig deps violate the
  single-binary policy.
- **Extend the `for` loop with ad-hoc conditionals** — rejected: reinvents a DAG
  engine badly; graph-flow is small, typed, and tested.
- **Do nothing** — acceptable short-term; linear pipelines cover the four
  templates. This ADR is **sequenced after** ADR-004 and may be deferred if GOAP
  plans stay linear in practice.

## Deferral decision (2026-06-03)

Re-examined immediately after ADR-004 shipped, per the gate above. Two facts make
graph-flow **dead machinery today**:

1. **GOAP plans are linear.** `orchestrate.run` produces a straight chain of
   archetypes; there is no branch for conditional edges to choose.
2. **No outcome signal to branch on.** `agent.spawn` always returns
   `status:"completed"` — there is no pass/fail/score a conditional edge (e.g.
   "if tests fail → coder") could read. Every condition would evaluate the same
   way, collapsing to the linear `for`-loop `orchestrate` already runs.

Also, the extractable core is **~3,200 LOC** (not the ~600 estimated), and
`context.rs` (1,061 LOC) would need splitting under the 500-line rule — a large
cost for zero current benefit.

**Prerequisite to revisit:** a real per-step **outcome signal** — agents (and
`orchestrate` steps) returning a structured `success`/`failure`/score, so a
conditional edge has something to test. Once that exists *and* a plan needs a
retry/branch, vendor `graph-flow`'s in-memory core (graph.rs + task.rs +
context.rs[split] + error.rs + storage.rs; drop `storage_postgres.rs` + `rig`).

**Update (2026-06-03):** the prerequisite **now exists** — ADR-009 added a real
`success`/`exit_code` outcome, and `orchestrate` already does the common case
(**stop-on-failure**) inline. graph-flow remains **deferred** until a plan needs
*more* than linear stop-on-failure (true conditional **retry/branch** loops, e.g.
"on test failure → back to coder, max 2×"). At that point the ~3,200-LOC vendor is
justified; until then inline handling is sufficient and leaner.

## Implementation note (2026-06-03)

After ADR-009 supplied the outcome signal, this shipped — but as a **lean
clean-room executor**, not a wholesale vendor of graph-flow. Vendoring its ~3,200
LOC would have imported PostgreSQL, the Rig LLM client, a 1,061-line `Context`
blob, and a `Session`/storage layer rUvOS does not use. Instead `substrate/
ruvos-graphflow` (~140 LOC, pure `std`) implements the *capability* — a typed
`FlowGraph` with `Always`/`OnSuccess`/`OnFailure` conditional edges + a bounded
reference driver — modeled on graph-flow's design and attributed to it.

`orchestrate.run` gains an opt-in `max_retries` (default `0`):
- `0` → the linear stop-on-failure pipeline (ADR-009), unchanged.
- `>0` → the plan is compiled to a graph (forward `OnSuccess` edges + an
  `OnFailure` edge from each step back to the nearest preceding `coder`, i.e.
  rework), executed with a per-node visit cap of `max_retries+1` and an overall
  step budget. A failed `tester` loops back to `coder` and retries, bounded.

This delivers conditional retry/rework — the real branching capability — in a
fraction of the code, honoring the zero-bloat discipline.

## Rollout

Implemented. Sequenced after the outcome-signal prerequisite (ADR-009).
(`docs/superpowers/plans/<date>-graphflow-orchestrate.md`). Gate on whether real
GOAP plans exhibit branching; if they stay linear, this stays deferred.
