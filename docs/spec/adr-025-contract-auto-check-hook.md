# ADR-025: Contract Manifest Auto-Check Post-Edit Hook

**Status:** Implemented (as a generic `post_write_check`, not a hardcoded hook)
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

### Implementation note (deviation from the original hooks_route design)

The shipped implementation does **not** hardcode the ruvos paths into a hook route,
because ruvos must remain **project-agnostic** (no awareness of `crates/ruvos-mcp/...`
or `docs/contracts/...` in its own code). Instead, `agent_exec` gained an optional,
fully caller-supplied `post_write_check` parameter:

```json
{
  "ops": [ /* ... */ ],
  "post_write_check": {
    "when_path_contains": "crates/ruvos-mcp/src/tools/",
    "when_path_ends_with": ".rs",
    "command": "ruvos",
    "args": ["contracts", "check", "docs/contracts/contract-manifest.json"],
    "cwd": "/home/lyle/dev/ruvos"
  }
}
```

Behaviour: if any successful `write_file`/`patch_file` op touched a path matching the
predicate, the command runs **once** after all ops complete. Non-zero exit attaches a
non-blocking `post_write_check: { ran, drift: true, exit_code, warning }` to the
response; `success` is never flipped. When the parameter is absent or no write matched,
the field is omitted entirely. The watched glob, check command, and manifest path are
all supplied by the caller — any project can use the mechanism; ruvos stays agnostic.

This also avoids the glob-support enhancement to `hooks_route` the original design
required. **Validated on first run:** the guard immediately surfaced real drift —
`ruvos_server_reload` (ADR-033) had never been added to the manifest (53→54 tools).

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
