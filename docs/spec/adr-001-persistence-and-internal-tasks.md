# ADR-001: redb + `.rvf` persistence, internal task ownership, and `gov.events`

**Status:** Accepted (2026-06-03)
**Supersedes:** scope-ledger-v1.md §1 decision to delete `task.*` (partial — see below)

## Context

rUvOS needs persistent, queryable storage for agent state that can scale beyond
the flat-JSON files used in early phases (`agents.json` rewrites the whole file
on every write — O(n) per write, no indexed lookups). Two questions arose:

1. **Which storage engine?** A vendored crate (`ruv-swarm-persistence`) offered
   a complete SQLite-backed store. SQLite scales and queries well, but pulls a
   bundled **C** library (`libsqlite3-sys`), introduces a second storage
   paradigm alongside `.rvf`, and carries no provenance/signing.
2. **Should rUvOS own task state?** The scope ledger (§1) deleted `task.*`,
   reasoning that the host CLI (Claude Code's `TaskCreate`, etc.) owns tasks.

## Decision

### 1. Storage: redb + `.rvf`, not SQLite

Persist via **`ruvos-store`** (new crate):

- **redb** — a pure-Rust embedded database — is the live, queryable store
  (indexed lookups, range queries, race-safe write transactions). redb is
  already in the workspace via `ruvector-core`'s `storage` feature, so this adds
  no new heavyweight dependency and keeps the binary pure-Rust (no bundled C).
- **`.rvf`** — signed, witness-chained snapshots (`snapshot_to_rvf` /
  `restore_from_rvf`) provide provenance, tamper-evidence, backup, and
  portability, reusing the `rvf-crypto` witness chain + keyed-HMAC attestation
  already used by `ruvos-session`.

Split of responsibility: **redb = working desk (fast); `.rvf` = sealed archive
(trustworthy).**

`ruv-swarm-persistence` (SQLite) is **deleted** — its capability is fully
re-implemented in `ruvos-store` on redb, and SQLite contradicts the pure-Rust /
single-signed-artifact identity. The dormant SQLite `HookQueue` in `ruvos-hooks`
is also removed (it was never on the live MCP path).

### 2. Task ownership: internal-only, host still owns the user's task list

The `task.*` deletion **stands for user-facing tasks** — Claude Code (the host)
owns the task list the user interacts with. rUvOS does **not** duplicate it.

However, when rUvOS runs its **own internal swarms** (a coordinator spawning
sub-agents the host cannot see), it genuinely needs to track those sub-tasks.
`ruvos-store` therefore implements a full task lifecycle (create/claim/pending/
by-agent, race-safe claim), **used in-process by rUvOS's own orchestration
code** — but **NOT exposed as MCP tools.** Keeping it code-internal enforces the
"internal swarm tasks only" boundary structurally: no agent or user can misuse a
`task.*` tool to create a competing task list, because no such tool exists.

This avoids the two-sources-of-truth / drift problem while still giving the
internal coordinator real task state.

### 3. One new MCP tool: `gov.events`

Expose a **signed audit/event log** query: `gov.events` (events since a
timestamp / by agent / by type). This fits rUvOS's provenance identity ("what
happened, when, by whom") and duplicates nothing the host provides.

Tool count: **20 → 21**. Within the ledger's "stopping budget of 80, any new
tool requires an ADR" — this ADR is that justification.

Tasks, metrics, messages remain **storage-only** (no MCP tools). They are
available to in-process orchestration and can be exposed later via a follow-up
ADR if a concrete, non-duplicative need appears.

## Consequences

**Positive:**
- Pure-Rust binary, no bundled C; one storage engine family (redb + `.rvf`),
  consistent with the vector kernel.
- Scales: indexed/range queries and efficient appends replace whole-file rewrites.
- Provenance preserved: signed `.rvf` snapshots with witness chains.
- Clean ownership boundary: host owns user tasks; rUvOS owns internal swarm tasks
  (code-internal), no duplication.

**Negative / trade-offs:**
- `ruvos-store` re-implements the store on redb (one-time engineering cost) rather
  than reusing the SQLite crate as-is.
- Full task/metric/message capability exists but is not MCP-reachable yet; some
  may consider unexposed capability premature (mitigated: it backs internal
  orchestration and is cheap to keep).
- Reversing part of a prior scope decision sets a precedent; mitigated by
  requiring this same ADR process for any future tool additions.

## Alternatives considered

- **Wire `ruv-swarm-persistence` (SQLite) directly** — least effort, but bundled
  C dependency, no provenance, second storage paradigm. Rejected on purity grounds.
- **Keep flat JSON** — simplest, but does not scale (O(n) per write, no queries).
  Rejected.
- **Expose full `task.*` as MCP tools** — maximum capability but reintroduces the
  duplication/drift with the host's task list and risks scope creep. Rejected in
  favor of internal-only tasks.
