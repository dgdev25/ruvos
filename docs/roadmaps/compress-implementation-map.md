# Compress Implementation Map

This document captures the implementation layout for the frozen `compress`
baseline in rUvOS.

The public/runtime name is `compress` only. No `headroom` symbols, tool names,
CLI names, config keys, or docs strings are allowed in shipping code.

## Scope

Supported clients:
- Claude Code
- Codex CLI
- Gemini CLI

Supported runtime surfaces:
- `compress` crate
- `compress.run` MCP tool
- `ruvos compress` CLI
- session-backed original storage for compressed payloads
- no-`headroom` runtime guardrail

Explicitly out of scope:
- Cursor
- Aider
- Copilot CLI
- OpenClaw
- proxy mode
- plugin-wrapper compatibility layers
- any additional client adapters

## Top-Level Layout

### Crate

- `crates/ruvos-compress/`
  - `Cargo.toml`
  - `src/lib.rs`
  - `src/detect.rs`
  - `src/json.rs`
  - `src/text.rs`
  - `src/pipeline.rs`
  - `src/ccr.rs`
  - `benches/compress_baseline.rs`

### Session Persistence

- `crates/ruvos-session/src/compress.rs`
- `crates/ruvos-session/src/lib.rs`

### MCP

- `crates/ruvos-mcp/src/tools/compress.rs`
- `crates/ruvos-mcp/src/tools/mod.rs`
- `crates/ruvos-mcp/src/server.rs`

### CLI

- `crates/ruvos-cli/src/commands/compress.rs`
- `crates/ruvos-cli/src/commands/mod.rs`
- `crates/ruvos-cli/src/main.rs`
- `crates/ruvos-cli/tests/client_scope.rs`
- `crates/ruvos-cli/tests/no_headroom.rs`

### Docs

- `README.md`
- `docs/roadmaps/compress-baseline-checklist.md`
- `docs/contracts/contract-manifest.json`

## Public API

The `compress` crate exposes a small runtime surface:

- `compress_content`
- `compress_content_into_session`
- `detect_content_type`
- `store_original`
- `retrieve_original`
- `CompressionConfig`
- `CompressionResult`
- `ContentKind`

## Current Implementation State

The baseline is already implemented in code. The map below describes the
actual runtime layout currently present in the workspace:

- `crates/ruvos-compress/src/json.rs`
  - JSON array thinning with signal preservation.
  - Higher-priority retention for error, endpoint, route, path, status, and
    message-bearing items.
- `crates/ruvos-compress/src/text.rs`
  - Shared text/log line selection.
  - Extra context around stack traces and error clusters.
  - Code boundary preservation for code-like payloads.
- `crates/ruvos-compress/src/pipeline.rs`
  - Content-kind routing.
  - Token accounting.
  - Session-backed persistence trigger for changed payloads.
- `crates/ruvos-session/src/compress.rs`
  - Session-scoped original payload persistence and metadata storage.
- `crates/ruvos-mcp/src/tools/compress.rs`
  - `compress.run` MCP tool with optional `session_id`.
- `crates/ruvos-cli/src/commands/compress.rs`
  - `ruvos compress` command with stdin/file support, kind hints, raw output,
    and optional session persistence.
- `crates/ruvos-cli/tests/client_scope.rs`
  - Guards the supported client list and the dropped client list.
- `crates/ruvos-cli/tests/no_headroom.rs`
  - Guards runtime source paths against `headroom`.
- `docs/roadmaps/compress-preservation-report.md`
  - Dedicated evidence report for preservation and token reduction.

## Implementation Order

1. Add the `compress` crate and workspace entry.
2. Define the public API and content detection.
3. Implement JSON compression.
4. Implement text/log compression.
5. Implement code-aware compression.
6. Add session-backed original persistence.
7. Wire `compress.run` into MCP.
8. Wire `ruvos compress` into the CLI.
9. Add the no-`headroom` runtime guard.
10. Add client-scope and round-trip tests.
11. Benchmark the baseline and record metrics.
12. Refresh and verify the contract manifest.

## File-by-File Responsibilities

### `crates/ruvos-compress/src/lib.rs`

- Exposes the public API.
- Hosts crate-level tests for detection and session-backed persistence.

### `crates/ruvos-compress/src/detect.rs`

- Detects whether input is JSON, code, log, or plain text.

### `crates/ruvos-compress/src/json.rs`

- Compresses structured JSON content.
- Preserves higher-signal fields and sparse array entries.

### `crates/ruvos-compress/src/text.rs`

- Compresses log and text payloads.
- Preserves code boundaries for code-like content.
- Preserves nearby context around error and stack-trace lines.

### `crates/ruvos-compress/src/pipeline.rs`

- Coordinates detection, compression, token estimates, and session persistence.

### `crates/ruvos-compress/src/ccr.rs`

- Generates stable references for originals.
- Stores and retrieves originals via the session layer.

### `crates/ruvos-session/src/compress.rs`

- Persists compressed originals in `.rvf` session state.
- Loads originals back from the same session container.

### `crates/ruvos-mcp/src/tools/compress.rs`

- Implements the `compress.run` MCP tool.
- Supports optional `session_id` persistence.

### `crates/ruvos-mcp/src/server.rs`

- Applies compression to tool output before MCP serialization.
- Avoids recursive compression on `compress.*` tool output.

### `crates/ruvos-cli/src/commands/compress.rs`

- Implements the `ruvos compress` command.
- Supports stdin/file input, kind hints, and session persistence.

### `crates/ruvos-cli/tests/client_scope.rs`

- Verifies the frozen supported and dropped client list.
- Verifies the README reflects the frozen client scope.

### `crates/ruvos-cli/tests/no_headroom.rs`

- Guards runtime source paths against `headroom` references.

## Baseline Acceptance

The baseline is considered complete when:

- `compress` works end-to-end for Claude Code, Codex CLI, and Gemini CLI.
- Session-backed originals can be recovered from the same `.rvf` session.
- `compress.retrieve` remains session-only and is not exposed as a public tool.
- Runtime source paths remain free of `headroom`.
- The contract manifest matches the live registry.
- No new runtime surfaces are introduced without a separately approved scope item.

## Related Checklist

- [Compress Baseline Checklist](./compress-baseline-checklist.md)
