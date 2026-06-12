# ADR-037: Governance Re-baseline — File-Size Gate, LOC Budgets, Tool-Surface Source of Truth

**Status:** Accepted
**Date:** 2026-06-12

## Context

A codebase audit found the project's governance claims had drifted from
reality:

1. CLAUDE.md claimed a CI-enforced "all `.rs` files ≤ 500 lines" rule.
   **No such check existed anywhere** (CI, justfile, scripts), and 15 files
   violated it — the largest (`tools/swarm.rs`, 1,992 lines) by ~4x. This is
   the exact failure mode (v3's 5,331-line `hooks.ts`) the rule was written
   to prevent.
2. The scope ledger's "Total: 20 tools" reads as the current state, but the
   live registry ships 60 tools (each added via ADR-001…036, within the
   ledger's own "stopping budget: 80 ever").
3. Per-crate LOC budgets (ruvos-mcp ≤6k) no longer match reality
   (ruvos-mcp ≈ 24.4k LOC of source), and `ruvos-cve-lite` / `ruvos-compress`
   exist outside the original six-crate table.

A contract nobody enforces trains everyone to ignore all the contracts.

## Decision

1. **File-size gate, ratcheted.** `scripts/check-max-lines.sh` enforces
   ≤500 lines for every `.rs` file under `crates/`, with the 15 current
   violators grandfathered at their 2026-06-12 line counts. Grandfathered
   files may shrink but not grow; new files get no exceptions. Wired into
   `just ci` and the CI `workflow-contract` job. Splitting the grandfathered
   files is tracked as ongoing refactoring work, largest first.
2. **Tool surface source of truth.** The binding registry of tools is
   `docs/contracts/contract-manifest.json` (generated from the live
   registry, checked by `contracts check` + the
   `handler_registry_matches_tool_metadata` test). The scope ledger §1 list
   is the historical v1 baseline; the "stopping budget: 80 tools ever" and
   the one-ADR-per-tool rule remain binding.
3. **LOC budgets re-baselined.** Budgets now describe intent, measured by
   `find crates/<c>/src -name '*.rs' | xargs cat | wc -l`:

   | Crate | Old budget | Actual (2026-06-12) | New budget |
   |---|---|---|---|
   | ruvos-cli | 8k | 2.9k | 8k (unchanged) |
   | ruvos-mcp | 6k | 24.4k | 26k (hard cap; reduce via substrate delegation) |
   | ruvos-host | 6k | 0.4k | 6k (unchanged) |
   | ruvos-plugin-host | 4k | 0.6k | 4k (unchanged) |
   | ruvos-hooks | 3k | 0.4k | 3k (unchanged; logic largely lives in ruvos-mcp) |
   | ruvos-session | 3k | 0.7k | 3k (unchanged) |
   | ruvos-cve-lite | — | 2.4k | 4k (new; ADR-gated crate) |
   | ruvos-compress | — | 1.3k | 2k (new; ADR-gated crate) |

   The ruvos-mcp cap is deliberately tight against its actual size: growth
   must come with extraction into substrate crates or new ADR-gated crates,
   not accretion.

## Consequences

- CI now fails when any file grows past its cap — the contract is real again.
- Documentation (CLAUDE.md, scope ledger) points at the manifest for tool
  counts instead of hand-written numbers.
- The 15 grandfathered files are explicit, visible debt with a ratchet,
  instead of silent violations.
