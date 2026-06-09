# ADR-030: Extism WASM Sandboxing for the Plugin System

**Status:** Proposed (deferred after ADR-027 native plugin is validated)  
**Date:** 2026-06-09  
**Source:** github.com/extism/extism (BSD-3-Clause, 5,600 stars, active)

## Context

ADR-027 adopts native `.so`/`.dylib` plugins. Native plugins share the ruvos process address space, which means:
- A buggy plugin can crash ruvos
- A malicious plugin has full filesystem and network access
- Plugins must be compiled for the exact ruvos ABI version

For trusted internal plugins (forge-linter, go-linter) this is acceptable. For user-supplied or third-party plugins — which are the long-term goal — sandboxing is required.

**Extism** is the production-proven WASM plugin framework (BSD-3-Clause, permissive), used in Navidrome and Astrid OS for AI agents. It provides capability-gated host functions, multi-language plugin SDKs, and ABI stability via the WASM boundary.

## Decision

Adopt Extism as the WASM sandbox layer for the plugin system, implemented in two phases:

**Phase 1** (after ADR-027 native plugin validated): add `extism` as an optional ruvos dependency behind a `wasm-plugins` Cargo feature flag. Plugins can be either native (`.so`) or WASM (`.wasm`).

**Phase 2**: migrate `forge-linter` from native to WASM. This proves the WASM path and establishes the plugin SDK (Rust WASM plugin template).

Host functions exposed to WASM plugins:
- `run_command` (capability-gated: must declare in plugin manifest)
- `read_file` / `write_file` (path-scoped: plugin declares allowed paths)
- `memory_store` / `memory_retrieve` (ruvos memory, plugin-namespaced)

## Consequences

**Positive:**
- Memory-safe, process-isolated plugins — buggy plugin cannot crash ruvos
- ABI stability: WASM interface is version-stable; plugins don't need recompilation on ruvos upgrade
- Multi-language plugins: any language compiling to WASM can write ruvos plugins
- BSD-3-Clause is permissive and compatible with ruvos MIT distribution

**Trade-offs:**
- Adds Extism + WASM runtime dependency (~3MB compiled)
- WASM plugins have syscall overhead vs. native; acceptable for CI-speed tools, possibly not for hot-path hooks
- Deferred until native plugin path is validated (ADR-027 first)

## Alternatives Considered

- **Native `.so` only**: current ADR-027 plan. Simpler but unsafe for third-party plugins. Accepted for internal plugins only.
- **Build own WASM sandbox**: reimplementing what Extism already provides. Rejected.
