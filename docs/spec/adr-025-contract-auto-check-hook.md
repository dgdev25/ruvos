# ADR-025: Contract Manifest Auto-Check Post-Edit Hook

**Status:** Proposed  
**Date:** 2026-06-09  
**Gap:** #21 in gap-register.md

## Context

The contract manifest (`docs/contracts/contract-manifest.json`) drifts from the live tool registry whenever a new tool is added or an existing tool's schema changes. Drift is currently detected only at push time when `cargo test` runs `manifest_tool_count_matches_registry`. During Sprint 7-8 development, we had to manually remember to run `ruvos contracts generate` after every tool change.

Gap 7 and Gap 8 fixes both required manual manifest regeneration after the fact.

## Decision

Add a `post_edit` hook configuration entry that fires `ruvos contracts check` whenever any file matching `crates/ruvos-mcp/src/tools/**/*.rs` is written by `agent_exec`.

Implementation:
1. `agent_exec` `write_file` op emits a `post-edit` hook event (already done via `ruvos_hooks_post`)
2. The hook route checks if the written path matches `crates/ruvos-mcp/src/tools/`
3. If matched, automatically run `~/.local/bin/ruvos contracts check docs/contracts/contract-manifest.json`
4. If the check fails (drift detected), append a `contract_drift_warning` to the `agent_exec` response and continue (non-blocking)

The check is non-blocking — it warns but does not abort the exec op. A follow-up `ruvos contracts generate` op can be added to the exec batch explicitly when tools are being added.

## Consequences

**Positive:**
- Drift is detected immediately after every MCP source edit, not only at push time
- Warning in the `agent_exec` response prompts the next op to be `run_command: ruvos contracts generate`

**Trade-offs:**
- Adds ~50ms to every MCP source file write (the check is fast but not free)
- Hook route matching requires glob pattern support in `ruvos_hooks_route` (enhancement needed there too)

## Alternatives Considered

- **CI-only check**: only catch drift in GitHub Actions. Too late — drift causes test failures on push when it could be caught during development. Rejected.
- **Blocking check**: fail the `write_file` op if drift is detected. Overkill — drift is expected mid-task when adding a tool incrementally. Non-blocking warning is sufficient.
