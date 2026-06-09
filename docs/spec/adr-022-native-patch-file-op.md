# ADR-022: Native patch_file Op for agent_exec

**Status:** Proposed  
**Date:** 2026-06-09  
**Gap:** #18 in gap-register.md

## Context

Patching large Rust files in ruvos currently requires writing a Python script to the filesystem, executing it, and hoping the string replacements work correctly. This workaround was introduced in Sprint 7 because sending 400-line Rust files over the MCP wire for a 5-line change is wasteful and error-prone. The Python workaround introduced the double-brace `{{` / `}}` bug (Python format string escaping) that required a second patch script to fix.

## Decision

Add a `patch_file` op to `ruvos_agent_exec` supporting two modes:

**Mode 1 — String replace** (simplest, covers 80% of cases):
```json
{"op": "patch_file", "path": "src/lib.rs",
 "old": "fn foo() { bar() }",
 "new": "fn foo() { baz() }"}
```
Fails if `old` is not found exactly once in the file (prevents silent mis-patches).

**Mode 2 — Unified diff** (for complex multi-hunk changes):
```json
{"op": "patch_file", "path": "src/lib.rs",
 "diff": "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -10,3 +10,4 @@..."}
```
Applied via the `patch` crate (MIT). Fails if hunks do not apply cleanly.

Both modes fail atomically — the file is not written if the patch fails.

## Consequences

**Positive:**
- Eliminates Python patcher scripts entirely
- String-replace mode is simple and readable in agent_exec op lists
- No escaping issues — the content is JSON-encoded, not a Python string
- Atomic application prevents partial patches

**Trade-offs:**
- String-replace mode requires the `old` string to match exactly (whitespace included) — callers must read the current file content before patching
- Adds the `patch` crate dependency (MIT, minimal)

## Alternatives Considered

- **Python patcher scripts**: current workaround. Fragile, requires shell, introduced the double-brace bug. Rejected for permanent use.
- **JSON Patch (RFC 6902)**: path-based array/object patches. Designed for JSON documents, not source code. Rejected.
- **Always send full file content**: simple but expensive on large files and loses diff reviewability. Rejected.
