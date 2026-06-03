# ADR-002: `relay` — cross-instance coordination for independent Claude sessions

**Status:** Accepted (2026-06-03)
**Builds on:** ADR-001 (redb + `.rvf` persistence), scope-ledger-v1.md §1 (tool budget)

## Context

rUvOS can coordinate agents it spawns *inside a single orchestration*
(`agent.spawn`/`agent.message` + `ruv-swarm-transport`). It has **no** way to
coordinate **separate, independently-launched Claude Code instances** — e.g. one
terminal where Claude works on the backend and another where Claude works on the
frontend, each its own process, each unaware of the other.

That cross-instance axis is a genuine capability gap. A community project
demonstrated the value of it, but its architecture — a long-running broker
**daemon**, a **SQLite** backend, **per-second polling**, a listening **TCP
port**, and a TypeScript/Bun runtime — is the direct opposite of rUvOS's
architectural commitments:

- **No daemons** (Phase 4 removed the in-process daemon; bug #1766).
- **No SQLite** (removed entirely in ADR-001 — pure Rust, no bundled C).
- **stdio-only** (zero listening ports, zero network attack surface).
- **Single static Rust binary, zero Node.**

So we adopt the *capability*, not that implementation, and build it the rUvOS way.

## Decision

Add a new MCP tool domain, **`relay`**, for discovery and messaging between
independent rUvOS-connected Claude Code instances. A *relay node* is a live
instance that announces its presence and can pass messages to other nodes. (Name
chosen to avoid any reference to prior art and to distinguish it from rUvOS
`session.*`, which are saved `.rvf` work contexts — a relay node is a live
process, a session is a stored artifact.)

### Tools (3 new → total 21 → 24, within the 80-tool budget)

| Tool | Purpose |
|------|---------|
| `relay.announce` | This instance registers / refreshes its presence: `{id, pid, cwd, git_repo, summary, updated_at}`. Call on connect and when work focus changes. |
| `relay.list` | Discover other live instances (filterable by scope: machine / directory / repo). Returns each relay's metadata + freshness. Lazily prunes stale relays. |
| `relay.send` | Deliver a message to another instance by id (dropped into its file mailbox). |

A fourth capability — reading one's own inbox — is folded into `relay.list`
(it returns `inbox` for the calling instance) rather than a separate tool, to
keep the surface minimal. (If a dedicated `relay.check` proves necessary, it
goes through a follow-up ADR.)

### Mechanism — pure files, no daemon / port / DB

Everything lives under `$RUVOS_HOME/relays/` (disk is the source of truth, the
rUvOS model):

```
$RUVOS_HOME/relays/
├── <instance-id>.json          # presence record (heartbeat); mtime = liveness
└── <instance-id>.inbox/        # one file per inbound message
    └── <msg-id>.json
```

- **Discovery:** `relay.list` scans `relays/*.json`. Liveness is derived from
  `updated_at` / file mtime against a TTL (default 60s) — **no heartbeat thread,
  no daemon.** Stale relays (and their inboxes) are pruned lazily when `list`
  runs.
- **Messaging:** `relay.send` writes a message file into the recipient's
  `<id>.inbox/`. The recipient receives it the next time it calls `relay.list`
  (which drains + returns its inbox). **No polling loop, no socket, no port** —
  the agent checks when it's relevant, exactly like a human checking mail.
- **Identity:** each rUvOS MCP server process gets a stable instance id at
  startup (uuid persisted for the process lifetime).
- **Scope filtering:** `machine` (all local relays), `directory` (same cwd),
  `repo` (same git remote) — so a project only sees relevant collaborators.

### Provenance

Every `announce` and `send` is recorded via the existing signed audit log
(`gov.events`), so cross-instance activity is tamper-evident — a capability the
original design lacked.

### Optional future "live mode" (explicitly deferred)

If real-time (sub-second) delivery is ever required, an opt-in transport using
the already-vendored `ruv-swarm-transport` WebSocket could be added behind a flag
— but it introduces a listening port (network surface) and is **out of scope for
v1**. File mailboxes are the default and only mode initially.

## Consequences

**Positive**
- Fills the cross-instance coordination gap with zero new heavy dependencies.
- Stays pure-Rust, daemon-free, port-free, SQLite-free — consistent with ADR-001
  and the project identity.
- Reuses `$RUVOS_HOME` + `gov.events`; no new storage engine.
- Lazy staleness model needs no background threads.

**Negative / trade-offs**
- Delivery is pull-based (on `relay.list`), not push — acceptable for
  human-driven multi-session work, but not sub-second real-time.
- 3 more tools (21 → 24); modest scope growth, sanctioned via this ADR.
- File-mailbox throughput is fine for human-paced messaging, not high-volume
  machine chatter (which isn't the use case).

## Alternatives considered

- **Vendor the community project as-is** — rejected: daemon + SQLite + polling +
  TCP port + TS runtime undo ADR-001 and the no-daemon/stdio-only principles.
- **`ruv-swarm-transport` WebSocket for messaging** — rejected for v1: requires a
  listening port per instance (network surface). Kept as a deferred "live mode."
- **A shared redb DB across instances** — rejected: redb takes an exclusive
  process lock (one handle per file), so multiple instances can't share one redb
  database. File mailboxes sidestep the lock entirely.
- **Names considered:** `peer` (rejected — references prior art), `beacon`,
  `fleet`, `presence`. `relay` chosen — it reads naturally for both discovery and
  message-passing between nodes, and is clearly distinct from `session.*`.
