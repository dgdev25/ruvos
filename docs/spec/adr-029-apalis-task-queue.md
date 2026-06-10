# ADR-029: apalis as the Background Task Queue

**Status:** Implemented  
**Date:** 2026-06-09  
**Source:** github.com/geofmureithi/apalis (MIT, 1,200 stars, active)

## Context

Ruvos hooks (`hooks_pre`, `hooks_post`) fire synchronously on the MCP request path. For a large project, hooks need to do expensive work: emit governance events, trigger relay messages, run contract checks (ADR-025). Doing this synchronously adds latency to every tool call.

ForgeCMS will also need background task execution for: scheduled content publishing, cache invalidation, email notification, and image processing.

A reliable, MIT-licensed task queue with Tower middleware integration is needed for both projects.

## Decision

Adopt **apalis** (MIT, geofmureithi) as the background task queue for ruvos.

Integration points:
1. **Hooks**: `hooks_post` emits a task to the apalis queue instead of executing the hook handler inline. The queue processes hooks asynchronously at near-real-time latency without blocking the MCP response.
2. **Gov events**: governance event writes are queued (at-least-once delivery) to avoid blocking tool calls
3. **Contract check** (ADR-025): post-edit contract checks run as apalis tasks
4. **ForgeCMS**: a separate apalis worker process handles CMS background tasks (publish scheduler, cache flush, email queue)

Backend: SQLite via `apalis-sql` for ruvos (zero new infrastructure). ForgeCMS uses PostgreSQL backend (`apalis-sql` with `postgres` feature).

Tool count: no new MCP tools added. Apalis is infrastructure, not an exposed capability.

## Consequences

**Positive:**
- Hooks no longer block MCP response path — `hooks_post` call returns immediately
- Retry-on-failure built in: failed governance writes are retried automatically
- Tower-native: middleware pipeline matches ruvos's existing Axum/Tower architecture
- Monitoring UI (`apalis-web`) available for debugging task queues during development

**Trade-offs:**
- `apalis-sql` requires `tokio` runtime (already present) and `sqlx` (new dependency unless ruvos already uses it)
- Async hooks introduce eventual-consistency for governance: an event may not be readable via `gov_report` immediately after the tool call that triggered it. Add `flush_hooks: true` option to `hooks_post` for tests that need synchronous behaviour.

## Alternatives Considered

- **fang** (MIT, 716 stars): simpler but PostgreSQL-first; SQLite support is secondary. Rejected in favour of apalis's first-class SQLite.
- **Inline synchronous hooks**: current approach. Adds latency, blocks MCP responses. Rejected for production.
- **Tokio spawn**: fire-and-forget `tokio::spawn` for hooks. Simple but no retry, no monitoring, no persistence. Rejected.
