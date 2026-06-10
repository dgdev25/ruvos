# ADR-035: Auto-Swarm Creation in hooks_pre

**Status:** Implemented  
**Date:** 2026-06-10  
**Gap:** Automatic swarm lifecycle — no ADR gap number (new capability)

## Context

Every project using ruvos has a CLAUDE.md that tells agents to call
`ruvos_swarm_create` for multi-step tasks. But this is guidance, not
enforcement — an agent that skips it loses dependency ordering, cascade
failure propagation, and sprint metrics (ADR-023, ADR-024).

The `hooks_pre` tool already fires before every task (ADR-034). It has the
task description in its payload, a routing recommendation, and AISP
validation. It is the correct place to auto-create a swarm when the task
warrants one.

## Decision

When `hooks_pre` receives `kind=task`, run a **complexity probe** against the
task prose. If the probe signals a multi-step task AND no swarm is already
active in this session, call `swarm::create()` internally and return the
`swarm_id` in the response.

### Complexity probe — threshold signals (any two = complex)

| Signal | Matches |
|--------|---------|
| **Scope keyword** | refactor, migrate, implement, integrate, rewrite, scaffold, add feature, create module |
| **Multi-file indicator** | mentions 2+ file extensions (`.rs`, `.ts`, `.sql`, etc.), or words: "across", "throughout", "all modules", "each module" |
| **Length** | task prose ≥ 150 characters |
| **Explicit files** | ≥ 2 distinct path-like tokens (`foo/bar`, `src/`, `migrations/`) |

### Dedup — one swarm per session

Before creating, check `swarm::current()`. If a swarm is already live
(status=active), attach to it instead of creating a new one. Return both
`swarm_id` and `swarm_action: "attached"` vs `"created"`.

### Sprint ID naming

`auto-<unix-seconds>-<first-keyword>` — e.g. `auto-1749600000-refactor`.
Callers can override by passing `sprint_id` in the task payload.

### Schema additions to `hooks_pre`

New optional input field:
- `auto_swarm: bool` (default `true`) — set `false` to suppress auto-creation
  for lightweight tasks

New output fields (only present when `auto_swarm=true` and `kind=task`):
```json
{
  "swarm_id":     "swarm-uuid",
  "sprint_id":    "auto-1749600000-refactor",
  "swarm_action": "created" | "attached" | "skipped"
}
```

`skipped` means the probe found the task simple (below threshold).

## Consequences

**Positive:**
- Every complex task automatically has a swarm — dependency ordering,
  cascade failure, and sprint metrics work without manual setup
- Universal: applies to all projects via the globally-registered ruvos binary
- Opt-out per call with `auto_swarm: false`; opt-out globally via
  `~/.ruvos/hooks.json` `"auto_swarm": false`

**Trade-offs:**
- Lightweight single-file edits still call `swarm::create()` if they happen
  to trip two signals accidentally. The `skipped` fast-path keeps this cheap:
  the probe is O(n) string scan with no I/O.
- `swarm::current()` returns the last-written swarm, not a session-scoped
  one. In a session with multiple swarms this may re-attach to a completed
  one. Mitigation: check `status == "active"` before attaching.

## Alternatives Considered

- **SessionStart skill per project**: possible today but opt-in per project,
  not universal. Rejected in favour of hooks_pre which is always on.
- **Explicit CLAUDE.md guidance only**: current state. Does not enforce
  behaviour. Rejected.
- **New MCP tool `swarm.auto`**: unnecessary indirection; hooks_pre is already
  the pre-task gate. Rejected.
