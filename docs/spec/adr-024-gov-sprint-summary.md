# ADR-024: gov_sprint_summary Tool

**Status:** Implemented  
**Date:** 2026-06-09  
**Gap:** #20 in gap-register.md

## Context

`ruvos_gov_report` returns a raw event dump. After each sprint (7, 8, 9) sprint progress was manually tracked in memory files and markdown docs because the governance layer provided no structured summary. A 297-test pass count required manually running vitest and reading the output.

For a large project with 12+ sprints, manual progress tracking is not sustainable.

## Decision

Add `ruvos_gov_sprint_summary` tool that accepts a `sprint_id` (string, free-form tag applied to swarm/tasks during a sprint) and returns:

```json
{
  "sprint_id": "sprint-9",
  "duration_ms": 1840000,
  "agents_used": ["coder", "tester"],
  "tasks": {"completed": 4, "failed": 0, "total": 4},
  "files_written": ["editorShell.ts", "sprint9.test.ts", "ForgeEditor.tsx"],
  "commands_run": [{"cmd": "vitest run", "exit_code": 0, "duration_ms": 660}],
  "test_delta": {"before": 297, "after": 329, "added": 32},
  "commits": ["4707ca6 feat(sprint9): editor shell UX"]
}
```

The `sprint_id` tag is applied at swarm creation (`ruvos_swarm_create` gains an optional `sprint_id` field) and propagated to all tasks and exec ops within that swarm. The governance event log already captures all raw events; this tool aggregates them.

## Consequences

**Positive:**
- Replaces manual progress tracking in docs and memory files
- Test delta (`before` / `after`) confirms each sprint's quality impact without manually diffing
- Commit list in summary closes the loop between swarm activity and git history

**Trade-offs:**
- Requires `sprint_id` tagging to be applied consistently at swarm creation — opt-in, not retroactive
- `test_delta` requires a baseline test count captured before the sprint begins (add `baseline_tests` field to sprint metadata at swarm creation)

## Alternatives Considered

- **Manual docs**: current approach. Does not scale past 12 sprints. Rejected.
- **`gov_report` with filtering**: possible today but returns raw events, not aggregated metrics. Too much parsing burden on the caller.
