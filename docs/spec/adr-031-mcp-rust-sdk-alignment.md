# ADR-031: Align ruvos Protocol Layer with modelcontextprotocol/rust-sdk

**Status:** Proposed  
**Date:** 2026-06-09  
**Source:** github.com/modelcontextprotocol/rust-sdk (MIT, ~3,500 stars, official Anthropic, v1.7.0 May 2026)

## Context

Ruvos implements its own JSON-RPC/MCP message framing, tool schema serialisation, and stdio/SSE transport. This was appropriate when the official Rust SDK did not exist. As of May 2026, the official `modelcontextprotocol/rust-sdk` (MIT) is at v1.7.0 with stable tool macros, JSON Schema validation, OAuth, progress tracking, and task cancellation.

Maintaining a bespoke MCP protocol implementation carries ongoing cost: schema drift, transport bugs, compatibility issues with new MCP versions (2025-11-25 protocol version not yet in ruvos). The official SDK is the canonical reference and is likely to track the spec faster than ruvos can.

## Decision

Adopt `modelcontextprotocol/rust-sdk` as the protocol and transport layer for ruvos, replacing the bespoke implementation.

Migration approach:
1. Add `mcp-core` and `mcp-server` crates from the SDK as dependencies
2. Replace ruvos's `RuvosToolHandler` trait registration with the SDK's `#[tool]` macro
3. Replace the stdio transport layer with `mcp-server`'s built-in stdio/SSE transports
4. Keep all 53 ruvos tool implementations unchanged — only the handler registration and serialisation layer changes
5. Update contract manifest generation to use the SDK's schema introspection

The ruvos tool logic (swarm, memory, relay, gov, hooks, etc.) remains entirely in ruvos crates. The SDK provides only the protocol boundary.

## Consequences

**Positive:**
- MCP spec compatibility maintained automatically as the SDK tracks protocol versions
- Tool macros reduce boilerplate: `#[tool]` generates JSON Schema from Rust types
- OAuth and progress tracking features become available for free
- Reduces maintenance burden on ruvos's protocol layer

**Trade-offs:**
- Migration effort: 53 handlers must be re-registered using SDK macros. Estimated 2-3 days.
- SDK dependency ties ruvos to Anthropic's release cadence for protocol updates
- The SDK's `#[tool]` macro may conflict with ruvos's current `schema()` / `execute()` trait pattern — an adapter layer may be needed

## Alternatives Considered

- **Keep bespoke implementation**: zero migration cost but growing spec drift risk. Rejected for long-term maintenance.
- **rust-mcp-stack/rust-mcp-schema only** (MIT, 74 stars): lighter-weight schema types without the full SDK. Acceptable if only schema alignment is needed, not transport. Deferred as step 1 of a phased migration.
