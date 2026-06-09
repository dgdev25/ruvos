# ADR-033: Atomic Binary Install, Structured Cargo Ops, and Server Self-Reload

**Status:** Implemented  
**Date:** 2026-06-09  
**Commit:** f26fea1

## Context

Three recurring friction points emerged after ADR-019 was shipped:

1. **`cp` blocked by "text file busy"** — overwriting the ruvos binary while the MCP server process holds it open fails at the OS level. The workaround (`cp → mv` atomic rename) was manual and error-prone.
2. **New binary only activates after Claude Code restart** — after a successful build and install, the live MCP server continues running the old binary. There was no in-process mechanism to replace it.
3. **`cargo check`/`cargo build`/`cargo test` routed through `run_command`** — possible but returned raw text; callers had to parse compiler output manually.

## Decision

### `install_binary` op (in `agent_exec`)

A new op that:
- Copies `src` to `<dest>.tmp_<pid>` (safe even if `dest` is held open)
- Copies permissions from `src` so the installed binary stays executable
- Renames `<dest>.tmp_<pid>` → `dest` atomically
- Returns `{ status, src, dest, bytes }` on success

This makes the build→install step a single ruvos op: no manual `cp`/`mv`, no "text file busy".

### `cargo_check` / `cargo_build` / `cargo_test` ops (in `agent_exec`)

Three new ops that run cargo subcommands and return **structured JSON**:

```json
{
  "status": "ok" | "error",
  "exit_code": 0,
  "success": true,
  "errors": 0,
  "warnings": 2,
  "stdout": "...",
  "stderr": "...",
  "test_summary": "test result: ok. 193 passed; 0 failed"
}
```

Supported params: `manifest_path`, `package` (-p flag), `release` (cargo_build only), `test_filter` (cargo_test only).

Error/warning counts are extracted by scanning for `error[` and `warning[` prefixed lines in the combined output. `test_summary` is the `test result:` line from cargo test output.

### `ruvos_server_reload` tool

A new top-level MCP tool that calls `execve(2)` to replace the running server process in-place:
- Reads `current_exe()` + `argv[1..]`
- Calls `std::process::Command::new(exe).args(args).exec()`
- The new binary inherits stdin/stdout; the MCP client session continues uninterrupted
- If `exec` fails (e.g. binary not found), returns an error — does **not** terminate the existing server

Usage after a build+install:
```
agent_exec: cargo_build (release) → install_binary → ruvos_server_reload
```
No Claude Code restart needed.

## Consequences

**Positive:**
- Binary install is now a single ruvos op — fully automatable in an orchestrate pipeline
- Structured cargo output enables downstream agents to branch on `success` / `errors` without text parsing
- `server_reload` closes the restart friction loop permanently; new features are live the moment the binary is installed

**Trade-offs:**
- `execve` reload drops all in-flight async tasks (journals, swarm state held in RAM). Stateful operations should complete before calling `server_reload`. Persistent state (journals, slots, memory) survives because it is on disk.
- `install_binary` does not verify the binary is a valid ELF/executable before rename — callers should run `cargo_build` first and check `success: true`.

## Files Changed

- `crates/ruvos-mcp/src/tools/agent_exec.rs` — added `install_binary`, `cargo_check`, `cargo_build`, `cargo_test` ops
- `crates/ruvos-mcp/src/tools/server_reload.rs` — new file, `ServerReloadHandler`
- `crates/ruvos-mcp/src/tools/mod.rs` — registered `ServerReloadHandler`, added `tool_registry()` entry
