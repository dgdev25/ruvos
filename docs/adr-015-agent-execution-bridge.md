# ADR-015: Agent Execution Bridge

**Status:** Accepted  
**Date:** 2026-06-09

## Context

Ruvos agents (via `ruvos_agent_spawn` and `ruvos_orchestrate_run`) currently produce
markdown artifacts only. They cannot write files, run shell commands, or touch git.
This is Gaps 1–3 in the gap register. Claude Code acts as the manual executor —
reading artifacts and performing the actions itself.

The ruflo architecture closes this loop differently: spawned agents ARE mini Claude
instances (WASM sandbox or cloud managed containers) that autonomously call tools.
We want to move ruvos toward that model using infrastructure already present in the
codebase.

## Decision

Implement a three-layer execution bridge in order of increasing capability:

### Layer A — Explicit tool_plan in coordinator_steps

`orchestrate_run` already returns `coordinator_steps`. Extend each step with a
`tool_plan` field: an ordered list of `ToolOp` objects that describe exactly what
the MCP host (Claude Code) should do after running inference for that step.

Operations: `write_file`, `read_file`, `run_command`, `git_op`.

This is a **data-only change** — ruvos remains the planner, Claude Code remains the
executor, but the execution intent is now explicit and machine-readable rather than
implied by the archetype name.

### Layer B — ruvos_agent_exec MCP tool

A new tool that executes a list of `ExecOp`s directly inside ruvos using
`PluginExecutor` (already exists). Optional `sandbox: true` mode creates a temp
working directory and runs all operations relative to it, giving OS-level isolation
without a WASM runtime dependency.

Operations exposed: `write_file`, `read_file`, `run_command`, `git_op`.

This closes Gaps 1–3: ruvos can now write `.ts` files, run `cargo test`/`vitest`,
and perform `git add/commit` — all from a single MCP tool call.

### Layer C — ruvos daemon relay listener

A new `ruvos daemon watch` subcommand that runs a persistent background process:

1. Announces presence on the relay bus
2. Polls the relay inbox for incoming task messages
3. Dispatches `exec` messages to `agent_exec`
4. Routes `agent` messages through the `InProcessTransport` message bus
5. Reports results back via relay send and `ruvos_memory_store`

This is the ruflo coordinator pattern: a real daemon that picks up tasks from a
message bus and executes them with full tool access.

## Consequences

- Closes Gaps 1–3 completely (Layer B)
- Makes orchestration plans machine-executable without human mediation (Layer A)
- Enables multi-instance Claude Code coordination via the relay bus (Layer C)
- Ruvos remains project-agnostic — no ForgeCMS-specific logic enters any layer
- No Anthropic API key, no `claude -p` subprocess, no WASM runtime dependency added
- `PluginExecutor` (which already uses `tokio::process::Command`) is the execution engine
