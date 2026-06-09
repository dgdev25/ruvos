# ADR-028: gov_issues Tool — Rewrite of beads_rust

**Status:** Proposed  
**Date:** 2026-06-09  
**Source:** `/mnt/datadisk/repos/beads_rust` (Rust, ~197 lines, MIT)

## Context

Large project builds generate issues: bugs found during test runs, TODOs from code review, deferred tasks from sprint planning. These are currently tracked in gap-register.md (for ruvos) and docs/parity-plan.md (for ForgeCMS) as manual markdown edits.

`beads_rust` is a non-invasive SQLite+JSONL issue tracker designed for agent-first workflows with `--json` output. It is already Rust, already MIT, and closely matches what ruvos's governance domain needs.

## Decision

Integrate `beads_rust` into ruvos as the `gov.issues` domain, exposing four MCP tools:

- `ruvos_gov_issue_create`: create an issue with title, body, labels, sprint_id, priority
- `ruvos_gov_issue_update`: update status (`open` → `in_progress` → `closed`) and add comments
- `ruvos_gov_issue_list`: list issues by sprint, label, or status; returns JSON
- `ruvos_gov_issue_get`: get a single issue by ID with full comment history

Storage: SQLite via `rusqlite` (already a ruvos dependency via `beads_rust`). Issues are stored in `~/.ruvos/issues.db`.

The rewrite from `beads_rust` is minimal — wrap its core SQLite schema and CRUD logic behind the standard `RuvosToolHandler` trait. Estimated: ~300 lines of new Rust in `crates/ruvos-mcp/src/tools/issues.rs`.

This adds 4 tools: total moves from 53 → 57. Requires contract manifest regeneration (ADR-025 auto-check catches this).

## Consequences

**Positive:**
- Issues are tracked within ruvos, not in external markdown — agents can create issues programmatically during sprint runs
- `gov_sprint_summary` (ADR-024) can include `open_issues` in its output
- Essentially free reuse: beads_rust is already the right shape

**Trade-offs:**
- Adds 4 tools (contract manifest must be regenerated)
- `rusqlite` dependency is already present if beads_rust uses it; if not, adds a new C dependency (mitigated: rusqlite is ubiquitous in Rust CMS/governance work)

## Alternatives Considered

- **Keep markdown files**: current approach. Not machine-readable; agents cannot query them. Rejected.
- **redb for issue storage**: consistent with ADR-001 storage choice. However beads_rust already has a working SQLite schema; rewriting to redb for purity is premature. Can migrate later.
