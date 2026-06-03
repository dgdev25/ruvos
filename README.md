# rUvOS — The Agentic Operating System

rUvOS is a Rust-native agent orchestration system built on the RuVector substrate.

- **RuVector** is the kernel: self-learning vector search, graph RAG, local LLM inference, cryptographic state containers
- **rUvOS** is the shell: agent orchestration, multi-CLI support (Claude Code, Codex, Gemini), plugin system, hooks

**Status:** v4.0.0-rc.1 — all phases complete; 20 MCP tools implemented with real persistence. See docs/spec/scope-ledger-v1.md for architecture.

## Quick Start

```bash
cargo build --release
./target/release/ruvos mcp serve
```

## Development

- **Workspace structure:** `crates/` (rUvOS orchestration), `substrate/` (RuVector kernel)
- **Scope contract:** 20 MCP tools, 12 agent archetypes, 8 hooks, ≤30k rUvOS LOC
- **File size limit:** all .rs files ≤500 lines (enforced in CI)
- **Contributing:** See CLAUDE.md for development guidelines

## Phase Timeline

| Phase | What | ETA |
|-------|------|-----|
| **0** | Workspace scaffolding (you are here) | 3-5 days |
| **1** | Merge substrates, CI green | 1 week |
| **2** | MCP server + hello-world tool | 1 week |
| **3** | Plugin host + skill compatibility | 1 week |
| **4** | Hooks + SQLite queue | 2 weeks |
| **5** | Memory + session (.rvf) | 2 weeks |
| **6** | CliHost adapters (Claude + Codex) | 2 weeks |
| **7** | Cutover + deprecation | 1 week |

See docs/superpowers/specs/ and docs/spec/ for detailed planning.

## License

MIT
