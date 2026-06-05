# Compress Baseline Checklist

Scope is frozen.

Do not add new compression features, new client support, or new integration surfaces until this checklist is complete and the baseline is accepted.

## Current Baseline Scope

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

Frozen out of scope:
- Cursor
- Aider
- Copilot CLI
- OpenClaw
- proxy mode
- plugin-wrapper compatibility layers
- any new client adapters

## Completed

- [x] Add the `compress` crate and workspace entry
- [x] Expose a public `compress` API surface
- [x] Add basic content detection
- [x] Add basic JSON compression
- [x] Add basic text/log compression
- [x] Add a `compress.run` MCP tool
- [x] Add a `ruvos compress` CLI command
- [x] Compress MCP tool output in the server path
- [x] Persist compressed originals into session state when a session id is provided
- [x] Add a CLI round-trip test for session-backed compression
- [x] Add an MCP round-trip test for session-backed compression
- [x] Add baseline benchmark coverage for JSON, log, and code payloads
- [x] Record baseline metrics for JSON, log, and code compression
- [x] Refresh the live contract manifest
- [x] Add a runtime-source guard against `headroom`
- [x] Verify the session-backed original storage path for JSON, code, and text payloads
- [x] Add docs/help text that describes only the frozen baseline scope
- [x] Add CI coverage for the frozen client list and dropped client list
- [x] Add regression tests to ensure no new runtime `headroom` references are introduced
- [x] Validate the contract manifest after compression-related changes
- [x] Decide `compress.retrieve` remains session-only and does not become a public tool

## Remaining

## Baseline Metrics

Captured from `cargo bench -p compress --bench compress_baseline -- --noplot --sample-size 10 --warm-up-time 1 --measurement-time 1`:

- `compress_json`: `108.07 µs` to `108.67 µs`
- `compress_log`: `48.975 µs` to `49.140 µs`
- `compress_code`: `18.043 µs` to `18.392 µs`

Notes:
- These are execution-time baselines only.
- Preservation and token-reduction evidence is captured in
  [compress-preservation-report.md](./compress-preservation-report.md).

## Acceptance Criteria

- The baseline works end-to-end for the supported clients without requiring any new feature additions.
- Compression results are deterministic enough to compare across runs.
- Session-backed originals can be recovered from the same `.rvf` session that received them.
- The runtime source tree remains free of `headroom` references.
- Any future feature work must be added as a separate, explicitly approved checklist item.
