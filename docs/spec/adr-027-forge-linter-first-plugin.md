# ADR-027: forge-linter as the First Real Plugin

**Status:** Proposed  
**Date:** 2026-06-09  
**Gap:** #5 in gap-register.md

## Context

`ruvos_plugin_list` returns an empty array. `ruvos_plugin_invoke` has never been exercised with a real plugin. The plugin subsystem (dynamic dispatch via trait objects) is untested end-to-end. Until there is at least one real plugin, the entire plugin architecture is theoretical.

Building `forge-linter` as the first plugin serves two purposes: proves the plugin system works, and provides a genuinely useful tool for ForgeCMS development (runs `cargo clippy` + `cargo fmt --check` on demand from an agent).

## Decision

Build `forge-linter` as a native ruvos plugin (a `.so`/`.dylib` implementing the `RuvosPlugin` trait):

```
Inputs:  { path: string, fix: bool }
Outputs: { warnings: [{file, line, message}], errors: [...], fixed: bool }
```

Internals:
1. Runs `cargo clippy --message-format=json` on `path`
2. Optionally runs `cargo fmt` (when `fix: true`)
3. Parses JSON output into structured `warnings`/`errors`
4. Returns structured result via `plugin_invoke` response

Registration: the plugin is loaded at ruvos startup if `~/.ruvos/plugins/forge-linter.{so,dylib}` exists. `ruvos_plugin_list` then returns it.

If the Extism WASM sandbox (ADR-030) is adopted later, `forge-linter` becomes the first WASM plugin as the migration proof.

## Consequences

**Positive:**
- Proves plugin dynamic dispatch end-to-end (load → list → invoke)
- `agent_exec` coder pipelines can call `plugin_invoke: forge-linter` instead of `run_command: cargo clippy` — structured output, no shell parsing
- Establishes the pattern for future plugins (go-linter, eslint, vitest-runner)

**Trade-offs:**
- Native `.so` plugins require ABI stability — plugin must be compiled against the same ruvos version. WASM would eliminate this, but WASM adds complexity deferred to ADR-030.
- Platform-specific: `.so` on Linux, `.dylib` on macOS — CI must build both.

## Alternatives Considered

- **`run_command` directly**: current approach. Works but returns raw stdout/stderr, requiring the caller to parse clippy JSON output. Rejected in favour of structured plugin output.
- **Skip plugins entirely**: use `agent_exec` run_command for everything. Viable short-term but leaves the plugin system permanently unvalidated. Rejected.
