# Headroom Recode Plan

This roadmap identifies the biggest `headroom` subsystems that are actually
worth recoding into `ruvos`.

Baseline constraint:
- Public/runtime naming stays `compress` or native `ruvos` names only.
- No `headroom` symbols, CLI names, tool names, config keys, or docs strings
  should ship in runtime code.
- Do not duplicate systems that `ruvos` already owns unless the port adds clear
  value.

## Already Completed In `ruvos`

- Native `compress` crate and runtime API
- `compress.run` MCP tool
- `ruvos compress` CLI command
- session-backed original storage
- no-`headroom` runtime guard
- compression baseline tests and evidence report

## Big Features Worth Recoding

### 1. Cache-Safe Proxy Core

Priority: Highest

What it is:
- Byte-faithful proxy handling for upstream AI requests and streaming
  responses.
- Live-zone-only compression so the cache hot zone stays untouched.
- SSE parsing that preserves wire-level correctness for streaming providers.

Why it matters:
- This is the main architectural difference between a simple compressor and a
  production-grade agent proxy.
- It prevents cache churn, preserves provider-specific wire formats, and makes
  compression safe under streaming workloads.

What to port:
- Byte-level SSE state machine
- Live-zone request rewriting
- Provider response passthrough rules
- Cache-safe header handling

Current `ruvos` status:
- Not part of the frozen `compress` baseline.
- Worth recoding only if `ruvos` grows a real proxy surface again.

### 2. CCR Reversible Retrieval Layer

Priority: High

What it is:
- Reversible compression with original retention and retrieval markers.
- Session-scoped or store-backed original lookup on demand.

Why it matters:
- Keeps compression lossless from the user’s perspective.
- Lets the system trim aggressively without discarding raw inputs.

What to port:
- Persistent original storage
- Retrieval marker generation
- On-demand recovery path
- Integrity checks for stored originals

Current `ruvos` status:
- Baseline session-backed original storage is already present.
- A broader public retrieval surface is intentionally not enabled.

### 3. Cache Stabilization and Tool Normalization

Priority: High

What it is:
- Deterministic ordering and normalization of tool definitions.
- Cache-control placement and prompt-cache key management.
- Other request-shape stabilizers that improve cache hit rates.

Why it matters:
- Often more impactful than raw compression because it improves provider cache
  reuse.
- Reduces token spend without changing user-visible behavior.

What to port:
- Tool sorting and schema normalization
- Automatic cache-control insertion rules
- Prompt cache key injection rules
- Volatility detection and warnings

Current `ruvos` status:
- Not yet recoded as a dedicated subsystem.

### 4. Provider-Native Routes and Auth-Mode Policy

Priority: High

What it is:
- Native handling for provider-specific APIs instead of lossy adapter layers.
- Policy differences between auth modes or account types.

Why it matters:
- Removes conversion loss.
- Lets the system choose the safest compression policy for each provider and
  deployment mode.

What to port:
- Native provider request handlers
- Auth-mode classification
- Policy gates per mode
- Provider-specific streaming rules

Current `ruvos` status:
- Not part of the current baseline.
- Would be a major expansion, not a small refactor.

### 5. Evaluation, Replay, and Regression Infrastructure

Priority: High

What it is:
- Benchmarks, replay tooling, and regression checks for compression and
  protocol behavior.

Why it matters:
- This is how the system stays trustworthy as the code grows.
- It turns the port from a one-time migration into a maintainable platform.

What to port:
- Benchmark suites for representative payloads
- Replayable protocol traces
- Savings and fidelity reports
- Automated regression checks for edge cases

Current `ruvos` status:
- Partial: `compress` already has baseline tests and a report.
- Good candidate for a broader cross-crate evaluation harness.

### 6. Cross-Agent Learning Signals

Priority: Medium

What it is:
- Learning from repeated tool outputs, failure patterns, and useful
compressions.
- A feedback loop that improves future compression choices.

Why it matters:
- Can improve compression quality over time.
- Helps route certain payload shapes to the right compressor.

What to port:
- Compression outcome telemetry
- Pattern scoring from repeated workloads
- Feedback loops for route selection

Current `ruvos` status:
- Implemented as a compression-learning bridge that writes outcomes into the
  existing `memory` and `intent` stores and emits runtime events.
- `ruvos` already has memory and intel subsystems, so the recode stays as a
  signal layer instead of a second semantic memory system.

## Proposed Implementation Order

1. Finish any remaining cache-safe proxy work only if `ruvos` reintroduces a
   real proxy surface.
2. Expand CCR support where it stays session-safe and does not create a second
   memory system.
3. Add cache stabilization and tool normalization if provider cache hit rate
   matters for the next stage.
4. Add provider-native routes and auth-mode policy only if the product scope
   includes those providers.
5. Expand benchmarking and replay tooling around the systems already in place.
6. Compression-learning signals are already in place; extend them only if they
   need to route into new policy or ranking logic.

## Explicitly Not Worth Recoding

- Python SDK wrappers and language bindings
- Wrapper-based client compatibility layers for extra agents that `ruvos`
  does not support
- Duplicate semantic memory storage
- Marketing/demo packaging that does not change runtime behavior

## Current `ruvos` Decision

The highest-value recode candidates are the proxy fidelity layer and any
provider-native transport work that is still intentionally out of scope.
CCR is already partially present and should be extended only where it stays
session-scoped. Cross-agent learning is now in place as a signal bridge and
should only be extended if it materially improves routing or policy selection.
