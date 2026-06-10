# ADR-028: gov_issues Tool — beads_rust (`br`) CLI Integration

**Status:** Implemented  
**Date:** 2026-06-09 (rewritten after reading beads_rust repo)  
**Source:** `/mnt/datadisk/repos/beads_rust` (`br`, ~20K lines Rust, MIT)

## Context

beads_rust is a complete, production-grade local issue tracker: ~20K lines of Rust, SQLite + JSONL storage, full CLI (`br`), dependency tracking, VCS integration, and `--json` output on every command. It is a Rust port of Steve Yegge's `beads`, frozen at the SQLite+JSONL architecture.

Key facts:
- Binary: `br` — already installable via `install.sh` or `cargo install`
- Storage: SQLite (`custom.db`) + JSONL export for git-friendly collaboration
- All commands accept `--json` for machine-readable output
- Agent integration built-in: `br agents --add` writes AGENTS.md instructions
- Dependency tracking: `br dep add <issue> <depends-on>`
- VCS-aware: syncs with git via `br sync`

Ruvos currently tracks issues in markdown files (`gap-register.md`, `docs/parity-plan.md`). These are not machine-readable and cannot be queried by agents programmatically.

## Decision

**Integrate `br` via CLI subprocess** — do not rewrite or embed the crate. beads_rust is a 20K-line codebase; vendoring it as a workspace crate adds significant maintenance burden. The `--json` flag makes CLI integration clean and stable.

Add a `gov_issues` domain with MCP tools that shell out to `br` via `PluginExecutor` (same pattern as `run_command` in `agent_exec`):

### Tools

| Tool | `br` command | Description |
|------|-------------|-------------|
| `ruvos_gov_issue_create` | `br create <title> --type <t> --priority <p> --json` | Create issue, return ID |
| `ruvos_gov_issue_list` | `br list --json [--status <s>] [--priority <p>]` | List issues with filters |
| `ruvos_gov_issue_show` | `br show <id> --json` | Full issue + comment history |
| `ruvos_gov_issue_close` | `br close <id> --json` | Close issue |
| `ruvos_gov_issue_search` | `br search <query> --json` | Full-text search |
| `ruvos_gov_issue_dep` | `br dep add <id> <dep-id>` | Add dependency between issues |

### `br` binary discovery

At startup (or first tool call), ruvos checks `$PATH` for `br`. If not found, returns an error pointing to `cargo install beads_rust` or the install script. No fallback: this is a hard dependency for the `gov_issues` domain.

### Storage location

`br` uses `.beads/` in the repo root by default. Ruvos passes `--db-path ~/.ruvos/issues.db` to keep issues in the ruvos data root, separate from any ForgeCMS or other project.

### Response format

All tools return parsed JSON from `br`'s `--json` output, re-wrapped in ruvos's standard response envelope:
```json
{
  "status": "ok",
  "issue": { "id": "bd-7f3a2c", "title": "...", "priority": 1, "status": "open" }
}
```

## Consequences

**Positive:**
- Zero rewrite cost — `br` is a proven, complete tool; ruvos just wraps it
- Agents can create, query, and close issues programmatically during sprint runs
- `gov_sprint_summary` (ADR-024) can pull `open_issues` count from `br list --json`
- Issues persist in SQLite between sessions; JSONL export enables git-based sharing
- `br dep add` enables dependency-aware sprint planning (unblock ordering)

**Trade-offs:**
- Runtime dependency on `br` binary in `$PATH` — must be documented in ruvos install guide
- CLI subprocess adds ~10–50ms per call; acceptable for governance tools (not hot path)
- `br` uses Rust nightly (rust-toolchain.toml pins nightly) — install via `cargo install` or binary download only, not stable toolchain
- `--db-path` flag may not exist in current `br` version; fallback is a per-project `.beads/` directory or a symlink

## Alternatives Considered

- **Embed beads_rust as workspace crate**: correct long-term, but 20K lines of nightly Rust with its own dependencies is significant maintenance. Deferred until beads_rust stabilises on stable Rust.
- **Keep markdown files**: not machine-readable; agents cannot query them programmatically. Rejected.
- **Write a minimal SQLite issue store from scratch (~300 lines)**: simpler, but discards beads_rust's proven schema, dependency tracking, and VCS sync. Rejected.
