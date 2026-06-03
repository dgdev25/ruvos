# ADR-008: `midstream` live stream observability for `hooks`/`intel`

**Status:** Accepted (2026-06-03)
**Amends:** scope-ledger-v1.md §1 (may add `hooks.observe`); adds substrate crate `ruvos-stream`
**Tier:** 2 · **Source:** rUvnet `midstream` (`midstreamer-temporal-compare`, `-scheduler`)

## Context

rUvOS only judges agent work **after the fact** (`hooks.post` on the final
outcome). It has no view of an agent's output **as it streams**, so it cannot
catch an agent drifting from spec, looping, or producing anomalous output until
it's done. The `CliHost` adapters (`ruvos-host`) already emit normalized
**`Output` events** during a run — a stream that nothing currently analyzes.

rUvnet's **midstream** is a pure-Rust (crates.io) workspace for **inflight
token-stream analysis**: `midstreamer-temporal-compare` (DTW similarity vs. a
reference trajectory), a nanosecond scheduler, and anomaly/attractor analysis.
Its QUIC/WASM crates are optional and **not** needed here.

## Decision

1. **Vendor the analysis core** into a new pure-Rust substrate crate
   **`substrate/ruvos-stream`** (clean-room, attributed, MIT): DTW
   `temporal-compare` + a streaming anomaly/drift detector. **Exclude** the QUIC
   transport and WASM crates. Deps kept minimal (no network).

2. **Feed it the `CliHost` `Output` event stream.** As an agent streams output,
   `ruvos-host` forwards chunks to a `ruvos-stream` analyzer that maintains a
   running drift/anomaly signal against the task's intent (or a reference
   trajectory recalled from `intel`).

3. **Surface signals two ways:**
   - **`intel`:** the per-run stream summary (drift score, anomalies) is recorded
     alongside the trajectory, enriching what `sona` learns.
   - **Optional new tool `hooks.observe`** (or a flag on an existing hook) that
     reports the live signal and can recommend **early-abort** when drift exceeds
     a threshold. **If a new tool is added, this ADR amends the ledger tool count
     24 → 25** and is gated on that decision; the no-new-tool variant (record to
     `intel` only) ships first.

## Consequences

- **+** Adds the "Observe the stream" stage to the workflow: catch derailment
  mid-flight, not post-mortem; richer learning signal for the flywheel.
- **+** Pure Rust; reuses the existing adapter `Output` events.
- **−** Requires plumbing the adapter stream into the analyzer (real-time path,
  more moving parts) and is the most **research-y** of the five (value depends on
  signal quality). Hence Tier 2 / sequenced last.
- **Zero-defect:** `ruvos-stream` ≤500-LOC files, workspace member; port
  midstream's DTW tests; deterministic drift-detector unit tests.

## Alternatives considered

- **Depend on `midstream` whole** — rejected: optional QUIC/WASM crates and a
  larger surface than needed; vendor the two analysis crates only.
- **Post-hoc analysis only** — that's today's `hooks.post`; misses the live value.
- **Build a bespoke detector** — rejected: DTW temporal-compare is exactly the
  rUvnet-original IP to reuse.

## Rollout

Sequenced **last** of the five (after GOAP + hybrid retrieval prove out).
Decision point before implementation: ship as **`intel`-only** (no tool count
change) vs. add **`hooks.observe`** (ledger 24 → 25). Plan to be written when
scheduled.
