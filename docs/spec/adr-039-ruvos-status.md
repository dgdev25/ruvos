# ADR-039: `ruvos status` — Human-Facing System Observability

**Status:** Implemented  
**Date:** 2026-06-12

## Context

All rUvOS observability is MCP-only: system health (`ruvos_gov_health`),
swarm state (`ruvos_gov_swarm_status`), the agent registry
(`ruvos_agent_status`), the signed event log (`ruvos_gov_events`), and relay
presence (`ruvos_relay_list`) are only reachable through an MCP client. A
human at a terminal has no way to answer "what is the system doing right
now?" without crafting JSON-RPC by hand. The agentic-OS roadmap (Phase 6)
calls for a visible system-state view.

## Decision

Add a read-only `ruvos status` CLI subcommand that is **pure presentation
over existing state** — zero new MCP tools, zero new state, zero writes.

1. **Reuse the exact MCP handlers in-process.**
   `crates/ruvos-cli/src/commands/status.rs::collect_status()` calls
   `GovHealthHandler`, `GovSwarmStatusHandler`, `AgentStatusHandler`,
   `GovEventsHandler` (limit 10), and `RelayListHandler` directly (ruvos-cli
   already depends on ruvos-mcp), merging their outputs into one JSON value
   with five sections: `health`, `swarm`, `agents`, `events`, `relays`.
2. **Graceful per-section degradation.** A handler error becomes an
   `{"error": ...}` marker for that section instead of failing the whole
   view, so a busy or broken store degrades one panel, not the command.
3. **Two output modes.** Default renders a sectioned human terminal view
   (`render_status`); `--json` emits the raw merged JSON for scripting.

## Consequences

- The CLI and the MCP surface can never disagree about system state — both
  read through the same handler code paths.
- No contract-manifest change: the 60-tool MCP surface is untouched; this is
  a CLI presentation layer only (consistent with the "one tool domain per
  scope" rule — no tool was added or sneaked in).
- Empty state is a first-class render: "no active swarm" / "none" rather
  than errors, so `ruvos status` is safe to run on a fresh install.
- Querying the agent registry emits an `agent.status.listed` audit event (a
  pre-existing handler side effect), so the events panel on an otherwise
  idle system may show the status query itself. Accepted: it is honest audit
  behavior of the shared handler.
