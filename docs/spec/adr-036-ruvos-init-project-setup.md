# ADR-036: ruvos init — Project CLAUDE.md Bootstrap

**Status:** Implemented  
**Date:** 2026-06-10

## Context

Every project using ruvos needs a CLAUDE.md section that:
1. Confirms ruvos is globally registered (no per-project wiring)
2. Instructs agents to call `hooks_pre` at task start (ADR-035 auto-swarm)
3. Lists the tool lookup table for the project's domain

Currently this section is manually written per project. New projects have no
guidance until a human writes it. The gap means agents in new projects silently
skip hooks_pre and never get auto-swarm or sprint metrics.

## Decision

Implement `ruvos init` (previously a no-op stub) as a project bootstrap command.

### What it does

Run from any project root:

```bash
ruvos init
```

1. **Detect project type** — scan CWD for `Cargo.toml` (Rust), `package.json`
   (Node/TS), `pyproject.toml` / `setup.py` (Python), `go.mod` (Go), or
   unknown.
2. **Find or create CLAUDE.md** — looks for `CLAUDE.md` in CWD. Creates it if
   absent.
3. **Idempotent injection** — checks for a `<!-- ruvos-managed -->` sentinel. If
   already present, updates the block in place. If absent, appends it.
4. **Create `.ruvos/` directory** — data root for sessions, memory, swarm state.
5. **Print a summary** — what was written, what was skipped.

### The injected block (language-aware)

The block is wrapped in HTML comments so it can be surgically updated:

```markdown
<!-- ruvos-managed: do not edit this block manually, run `ruvos init` to update -->

## rUvOS (globally registered — no setup needed)

**Before every non-trivial task**, call hooks_pre to trigger auto-swarm,
routing, and safety checks:

\```
ruvos_hooks_pre  kind=task  payload={"prompt": "<your task description>"}
\```

Use the returned `swarm_id` for all subsequent `ruvos_swarm_assign` calls.
Pass `auto_swarm: false` for single-file fixes.

| Situation | Tool |
|-----------|------|
| Save a decision or pattern for future sessions | `ruvos_memory_store` / `ruvos_memory_search` |
| Fork before a risky change | `ruvos_session_fork` |
| Resume interrupted work | `ruvos_session_resume` |
| Multi-step task with ordered stages | `ruvos_swarm_create` + `ruvos_swarm_assign` |
| Log a significant operation async | `ruvos_hooks_post` |
| Sprint retrospective | `ruvos_gov_sprint_summary` |
| Track a bug or task | `ruvos_gov_issue_create` |

<!-- end ruvos-managed -->
```

The block is identical across all project types. Language-specific additions
(e.g. Rust zero-warnings, Node async rules) are printed as suggestions but NOT
auto-written — the user owns their CLAUDE.md content outside the managed block.

### Flags

| Flag | Effect |
|------|--------|
| `--dry-run` | Print what would change without writing |
| `--force` | Overwrite the managed block even if unchanged |
| `--no-data-dir` | Skip `.ruvos/` directory creation |

## Consequences

**Positive:**
- Any developer can bootstrap a new project with one command
- Existing CLAUDE.md files are updated non-destructively (only the managed block changes)
- Idempotent — safe to re-run after ruvos updates

**Trade-offs:**
- HTML comment sentinels are visible in rendered markdown (minor cosmetic issue)
- Does not write language-specific rules — those stay manual to avoid overwriting user content
