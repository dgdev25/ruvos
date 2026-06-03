# ADR-006: SPARC as an `orchestrate` template

**Status:** Accepted (2026-06-03)
**Amends:** scope-ledger-v1.md §1 (`orchestrate.run` template set)
**Tier:** 2 · **Source:** rUvnet `SPARC` (methodology — encoded as data, no dependency)

## Context

rUvnet's **SPARC** is a development *methodology*: **S**pecification →
**P**seudocode → **A**rchitecture → **R**efinement → **C**ompletion. The repo is a
Python CLI wrapper, but the value is the **5-phase lifecycle**, not code. rUvOS's
`orchestrate.run` already executes ordered archetype pipelines from named
templates; SPARC is a natural, on-brand addition that costs almost nothing.

## Decision

Add a **`sparc`** template to `orchestrate.run`, encoded purely as data (a Rust
archetype sequence), mapping each SPARC phase to the best-fit archetype:

| SPARC phase | archetype |
|-------------|-----------|
| Specification | `researcher` |
| Pseudocode | `planner` |
| Architecture | `architect` |
| Refinement | `coder` |
| Completion | `tester` → `reviewer` |

i.e. `sparc` → `[researcher, planner, architect, coder, tester, reviewer]`.

Once ADR-004 (GOAP) lands, SPARC is additionally expressible as a **goal** whose
`desired_state` requires each phase's effect — letting the planner *derive* the
SPARC order rather than hardcode it. The static template is the v1 form; the
GOAP-goal form is the follow-on.

No new tool, no new dependency, no new crate — a one-line addition to the
`template()` match plus a test. The 24-tool count is unchanged.

## Consequences

- **+** A recognized, rigorous lifecycle available via the existing tool; free.
- **+** Forward-compatible with ADR-004 (becomes a planner goal).
- **−** One more archetype sequence to keep coherent; negligible.
- **Zero-defect:** add a `sparc_orchestration_runs_six_phases` test alongside the
  existing template tests.

## Alternatives considered

- **Depend on the SPARC CLI** — rejected: Python runtime, and there is no
  algorithm to import — only a sequence.
- **Ship as a plugin** — possible, but a first-class template is simpler and
  matches how `feature`/`bugfix`/etc. are already provided.

## Rollout

Trivial; folded into the GOAP plan's template work
(`docs/superpowers/plans/2026-06-03-goap-orchestrate.md`, Task: templates) or
shippable standalone as a single match-arm + test.
