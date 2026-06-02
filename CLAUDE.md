# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**rUvOS** (formerly Ruflo v4) is a Rust-native agent orchestration system being merged into the RuVector workspace. It's a ruthless rewrite of Ruflo from 631k TypeScript LOC + 323 MCP tools + 60+ agent types down to ~30k Rust LOC with 20 core tools and 12 agent archetypes.

**Core positioning:** RuVector is the self-learning vector + graph + local-AI substrate. Ruflo is the agent orchestration layer that runs on top of it. Single static binary (`ruflo`), zero Node.js runtime required.

## Architecture

### The Six New Crates (Ruflo's layer on top of RuVector)

| Crate | Budget | Purpose | Key files |
|-------|--------|---------|-----------|
| `ruflo-cli` | ≤8k LOC | clap-based CLI shell (`ruflo init`, `ruflo mcp`, `ruflo agent`) | — |
| `ruflo-mcp` | ≤6k LOC | JSON-RPC MCP server over stdio + the 20 tool handlers (memory, session, agent, hooks, intel, plugin, gov, workflow) | — |
| `ruflo-host` | ≤6k LOC | `CliHost` trait + Claude + Codex adapters, output normalizer for multi-CLI orchestration | — |
| `ruflo-plugin-host` | ≤4k LOC | Plugin discovery (markdown + YAML frontmatter), manifest parsing, shell command execution | — |
| `ruflo-hooks` | ≤3k LOC | 8 hooks (pre/post task, edit, command, session) + SONA learning integration | — |
| `ruflo-session` | ≤3k LOC | `.rvf` container write/read, fork (COW-branch), signature verification via `rvf-crypto` | — |

**Total: ≤30k LOC of new Rust.** Everything else is `use ruvector_*;` or `use sona::*;` or `use rvf::*;`.

### The 20 v1 MCP Tools (and 12 Agent Archetypes)

**Tools (by domain):**
- `memory.*` (4) — search, store, retrieve, list with MMR + recency
- `session.*` (3) — create, resume (restore from `.rvf`), fork
- `agent.*` (3) — spawn, status, message (for multi-agent swarms)
- `hooks.*` (3) — pre, post, route (unified hook dispatch + model recommendations)
- `intel.*` (2) — pattern_search (trajectory similarity), pattern_store (SONA learning)
- `plugin.*` (2) — list (discover), invoke (shell exec)
- `gov.*` (2) — witness_verify (`.rvf` signature chain), health (doctor / status)
- `workflow.*` (1) — run (orchestration templates: feature / bugfix / refactor / security)

**Agent archetypes:** coder, reviewer, tester, researcher, architect, planner, security, perf, devops, data, docs, coordinator (+ composable traits: `--trait=tdd`, `--trait=backend`, `--trait=frontend`, `--trait=mobile`, `--trait=ml`, `--trait=domain`, `--trait=cloud`, `--trait=db`, `--trait=audit`).

### Plugin Layout (Single Canonical Form)

```
./.ruflo/plugins/<name>/
├── plugin.toml              # Rust manifest
├── README.md
├── agents/*.md              # Claude Code agents (markdown + frontmatter)
├── skills/*/SKILL.md        # Claude Code skills
├── commands/*.md            # slash commands
└── hooks/*.toml             # hook bindings (optional)
```

Discovery order: project-local → user-global (`~/.ruflo/plugins/`) → env override → built-in registry.

## Development Workflow

### Phase Timeline

| Phase | What | Weeks |
|-------|------|-------|
| **0** | Scope ledger (this is it — you're in Phase 0 now) | 3–5 days |
| **1** | Merge into RuVector workspace, create 6 crate skeletons, CI green | 1 week |
| **2** | `ruflo mcp serve` ships hello-world tool to Claude Code, Codex CLI, Gemini CLI | 1 week |
| **3** | Plugin host (markdown discovery, shell exec, skill compatibility) | 1 week |
| **4** | 8 hooks + SQLite-backed queue (replaces in-process daemon, fixes Windows bug #1766) | 2 weeks |
| **5** | Memory + session (HNSW + RaBitQ + `.rvf` containers + witness chain) | 2 weeks |
| **6** | CliHost adapters (Claude + Codex normalized events; Gemini deferred) | 2 weeks |
| **7** | Cutover (npm shim ships, TS code → `legacy/`, v4 tag) | 1 week |

**Current state:** Phase 0 — scope ledger locked. No Rust yet.

### Build & Test (Phase 1+)

Once the Rust codebase exists:

```bash
# Build the binary
cargo build --release

# Run all tests
cargo test

# Run a single test
cargo test --lib <crate>::<module>::<test_name>

# Format + lint
cargo fmt && cargo clippy --all-targets --all-features

# Check code coverage (Phase 2+)
cargo tarpaulin --out Html

# Run doc tests
cargo test --doc
```

### Key Development Rules

**Enforced at CI time:**

1. **File size limit: all `.rs` files ≤ 500 lines.** Custom `--max-lines 500` check in CI. No exceptions. (Reason: the current codebase has files like `commands/hooks.ts` with 5,331 LOC — this is the contract to prevent that.)

2. **Crate budget: ≤30k LOC of new Rust total.** If a crate exceeds its per-crate budget, it's a smell that you're not using RuVector's substrate.

3. **One tool domain per scope.** If a tool isn't in the 20-tool list, it requires an ADR explaining which tool it replaces or what domain gap it fills. No "sneaking in" tools during phases.

4. **Plugin inventory:** 11 survivors out of 51 total. Provisional keep list in scope-ledger.md §5. Any plugin not on the list is deleted (not deferred).

5. **No legacy v2 compat.** `v2:migrate` command, pre-bash / post-bash aliases, all deleted. Clean break.

6. **Atomic refactors across repos.** Ruflo + RuVector changes land in one PR. One CI, one release cadence.

### Architecture Decisions Already Made

- **MCP server strategy:** Roll a thin JSON-RPC over `tokio::io::stdin/stdout` rather than wait on third-party crates. (Reason: MCP is small enough to own.)
- **Embeddings:** Don't ship local inference in v1. Call provider APIs / external commands. Defer `fastembed-rs` / `ort` / `candle` to v2.
- **State storage:** All state in `.rvf` containers + SQLite. Workers are stateless. (Reason: fixes the in-memory daemon graph state issue that caused Windows persistence bug #1766.)
- **Multi-CLI support:** `CliHost` trait + normalized event streams. Same orchestration binary runs under Claude Code, Codex CLI, and Gemini CLI. Three CLI hosts, one state store, one `.rvf` session format.
- **Plugin format:** Markdown + YAML frontmatter (same as Claude Code skills). No JS runtime, no WASM-for-plugins yet. Shell commands invoked via `tokio::process::Command`.

## Scope Contract (What's Deleted vs Current)

**Deleted entirely (not deferred):**

- All 323 → 20 MCP tools (aggressive cut; anything not on the list is removed)
- 60+ agent types → 12 archetypes + traits
- 4 plugin directories → 1 canonical layout
- 3 published npm packages → 1 artifact (`ruflo` binary + optional npm shim)
- 12 in-process daemon workers → SQLite durable queue (v2)
- 25 KB root `CLAUDE.md` + 21 KB `AGENTS.md` → 1 file ≤8 KB (autogenerated)
- `v2` compat hooks, marketplace UI, Flash Attention claims (until benchmarked), federation, DAA tools, coverage-aware routing

**Why the cuts matter:** The Prism consensus models flagged scope preservation as the difference between success and a stalled 80% rewrite. Everything not on the scope ledger is deleted, not deferred.

## Where to Look First

1. **Understand the scope:** `docs/spec/scope-ledger-v1.md` (open questions, tool list, archetype list, hook list, phase timeline).
2. **Understand the why:** `docs/rewrite-summary.md` (architecture rationale, decision tree, RuVector merge explanation, multi-CLI support model).
3. **Once Phase 1 starts:** Look for `crates/*/CLAUDE.md` (per-crate dev notes) and `scripts/inventory.rs` (which generates the root CLAUDE.md at build time).

## Common Workflows (Phase 1+)

### Adding a new tool

1. Add it to the ledger (`docs/spec/scope-ledger-v1.md` §1) via ADR, explaining which tool it replaces.
2. Implement the handler in `crates/ruflo-mcp/src/tools/<domain>.rs`.
3. Register it in the tool registry (built into MCP server startup).
4. Test with `ruflo mcp serve` hooked to Claude Code CLI.

### Adding a new agent archetype

1. Add it to the ledger via ADR (unlikely; the 12 are fixed).
2. Define the base prompt in `crates/ruflo-host/src/agents/archetypes/<name>.rs`.
3. Define traits that modify behavior in `crates/ruflo-host/src/agents/traits.rs`.
4. Test via `agent.spawn` MCP tool.

### Adding a plugin

1. Create plugin directory with `plugin.toml` + `README.md`.
2. Add agents/skills/commands as markdown files with YAML frontmatter.
3. Discover is automatic via `plugin.list` MCP tool.
4. Invoke via `plugin.invoke` (shell exec).

### Debugging the MCP handshake

Once `ruflo mcp serve` ships (Phase 2):

```bash
# In a separate terminal, start the MCP server
./target/release/ruflo mcp serve

# In Claude Code, hook it: claude mcp add ruflo -- ./target/release/ruflo mcp serve
# Then in the Claude Code REPL, call any tool and inspect its output
```

## Integration Checkpoints (Hard Stops)

**Phase 2 tripwire:** `ruflo mcp serve` must work end-to-end with the real Claude Code CLI on day 1. Integration drift with Claude Code is a failure mode both consensus models flagged. **Do not defer Claude Code testing to Phase 6.**

## Current Issues Driving the Rewrite

- **Windows daemon persistence bug #1766** — fixed by design (stateless workers, SQLite state store).
- **Headless race condition #2251** — fixed by moving state out of in-process memory.
- **Skipped integration tests #1872** — re-enabled in Phase 3+ once plugin host and hooks are live.
- **323 tools with aliases breaking discoverability** — ruthlessly cut to 20 + plugin space.
- **Three drifting npm packages from one repo** — one `ruflo` binary published, optional npm shim.

## Useful References

- **Scope ledger** (`docs/spec/scope-ledger-v1.md`) — the binding contract. Everything Phase 1+ must align with.
- **Rewrite summary** (`docs/rewrite-summary.md`) — decision rationale and consensus model input.
- **RuVector workspace** (`/mnt/datadisk/repos/rUvnet/RuVector`) — the substrate we're building on. 143 members, 136 crates. Use `default-members` to scope builds.
- **Cargo workspace policy:** Atomic refactors across Ruflo + RuVector land in one PR. One CI, one release cadence.

## Notes for Future Phases

- **Phase 1 focus:** Make sure `default-members` in the merged workspace only includes crates Ruflo consumes. Experimental RuVector crates (consciousness examples, quantum coherence) stay behind features. This keeps CI fast.
- **Phase 2 focus:** The MCP round-trip from real Claude Code CLI is the hard stop. Test with `claude mcp add ruflo -- ruflo mcp serve` and call tools from the Claude Code REPL.
- **Phase 3+ focus:** As plugins land, the canonical layout at `crates/ruflo-plugin-host/registry/<name>/` is the source of truth. IPFS is only a release-time CDN, not the source of truth.

---

## Phase 0 Completion (2026-06-02)

**Status:** ✅ Complete

Phase 0 established the rUvOS workspace structure with:

- ✅ RuVector dependency audit completed (`docs/spec/ruvector-curation.md`)
- ✅ 29 curated RuVector crates in `substrate/` (removed 33 variants/experimental)
- ✅ 6 Ruflo crates scaffolded with module structure matching scope ledger
- ✅ Root Cargo.toml with workspace + default-members properly scoped
- ✅ CI pipeline configured (build/clippy/fmt/test)
- ✅ All crates compile and pass checks

### Deliverables

1. **Workspace structure:** `crates/` (Ruflo) + `substrate/` (RuVector)
2. **29 curated RuVector crates:** core vector, SONA, RVF, RuVLLM, Raft, witness chain
3. **6 Ruflo crate scaffolds:** cli, mcp, host, plugin-host, hooks, session
4. **Module structure:** 20 MCP tools, 12 agent archetypes (stubs), 8 hooks documented
5. **CI pipeline:** GitHub Actions (build, clippy, fmt, test)
6. **Documentation:** Updated CLAUDE.md with Phase 0 notes

### Build Status

```
✓ cargo build --all-features — Finished in 5.55s
✓ cargo clippy --all-features -- -D warnings — Passed
✓ cargo fmt -- --check — Passed
✓ All 6 Ruflo crates recognized by workspace metadata
✓ Git working tree clean
```

### Crate Compilation Summary

All six Ruflo crates build cleanly together:
- `ruflo-cli` (8k LOC budget) — clap-based shell
- `ruflo-mcp` (6k LOC budget) — JSON-RPC server + 20 tools
- `ruflo-host` (6k LOC budget) — CliHost trait + adapters
- `ruflo-plugin-host` (4k LOC budget) — plugin discovery + manifest
- `ruflo-hooks` (3k LOC budget) — 8 hooks + SONA integration
- `ruflo-session` (3k LOC budget) — .rvf container + fork + crypto

### Next Steps

**Phase 1** will integrate the full RuVector workspace and prepare Phase 2's day-1 integration test (`ruflo mcp serve` → Claude Code CLI).
