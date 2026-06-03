# rUvOS — The Agentic Operating System

> ⚠️ **NOT BACKWARD COMPATIBLE WITH RUFLO v2/v3**
>
> **rUvOS v4 is a complete, clean-room Rust rewrite. It is _NOT_ compatible with
> the Ruflo v2/v3 npm CLI** (the TypeScript monolith — `ruflo` / `@claude-flow/cli`).
> There is **no migration path** and **no `v2:migrate`** — the clean break is
> intentional.
>
> **Running [`./setup.sh`](#install) uninstalls that v2/v3 npm CLI** (and clears
> its npm cache), replacing it with the single `ruvos` v4 binary. It does **not**
> touch the current `claude-flow` / `ruv-swarm` MCP servers or any Ruflo Claude
> Code plugins — those **coexist fine** with rUvOS (verified: separate namespaces,
> processes, and data dirs). When capabilities overlap, just name **rUvOS** in
> your request to route to it.

rUvOS is a Rust-native agent orchestration system. It runs as an **MCP server** that
plugs into Claude Code, Codex CLI, or Gemini CLI and gives them persistent memory,
resumable sessions, multi-agent coordination, a knowledge graph, safety guardrails,
and signed provenance — all from a **single static binary, zero Node.js, zero
external database.**

- **RuVector** is the kernel: self-learning vector search (HNSW + RaBitQ), graph,
  local-AI substrate, cryptographic `.rvf` state containers.
- **rUvOS** is the shell: agent orchestration, multi-CLI support, plugins, hooks.

> **The tagline:** *RuVector is the kernel, rUvOS is the shell.*

**Status:** `v4.0.0-rc.1` — production-grade. **24 MCP tools**, real persistence,
100% pure Rust (no SQLite, no bundled C), zero compiler/clippy warnings across the
whole workspace.

> ### 🙏 Built on the work of giants: [**rUv**](https://github.com/ruvnet)
>
> rUvOS exists because of **rUv (Reuven Cohen / [@ruvnet](https://github.com/ruvnet))** —
> the original creator and visionary behind **Ruflo / claude-flow**, **RuVector**, the
> **`.rvf`** format, **SONA**, **ruv-swarm**, **ruv-FANN**, and the entire agentic
> substrate this project stands on. Every kernel capability here — the vector search,
> the witness chains, the swarm transport, the self-optimizing learning — traces back
> to rUv's research and code. rUvOS is a Rust-native consolidation of that ecosystem;
> the hard, original ideas are his. **Huge thanks and full credit to rUv.** 🚀

---

## Table of contents

- [Install](#install)
- [How you actually use it](#how-you-actually-use-it-just-talk)
- [The 24 tools](#the-24-tools)
- [A natural-language session](#a-natural-language-session)
- [Feature reference — every tool, by example](#feature-reference--every-tool-by-example)
  - [memory](#memory--persistent-semantic-memory--knowledge-graph) ·
    [session](#session--resumable-signed-work-contexts) ·
    [agent](#agent--spawn-track-and-message-agents) ·
    [hooks](#hooks--lifecycle-hooks-safety--routing) ·
    [intel](#intel--sona-trajectory-learning) ·
    [plugin](#plugin--discover-and-run-plugins) ·
    [gov](#gov--health-provenance--audit) ·
    [relay](#relay--cross-instance-coordination) ·
    [orchestrate](#orchestrate--multi-agent-orchestration-templates)
- [Agent archetypes & traits](#agent-archetypes--traits)
- [Where your data lives](#where-your-data-lives)
- [Architecture](#architecture)
- [Development](#development)
- [Acknowledgments](#acknowledgments)
- [License](#license)

---

## Install

### One-shot (recommended)

Clone and run the installer — it does **everything**: builds the binary, **removes
any legacy Ruflo v2/v3**, installs `ruvos` onto your `PATH`, sets `RUVOS_HOME`,
registers the MCP server with Claude Code, and verifies the result.

```bash
git clone https://github.com/dgdev25/ruvos.git
cd ruvos
./setup.sh
```

Then:

1. **Restart Claude Code.** It loads MCP servers at startup, so a fresh start is
   required to pick up the newly-registered `ruvos` server (and any binary update).
2. Open a new terminal (so `PATH`/`RUVOS_HOME` take effect) and confirm:

```bash
claude mcp list          # ruvos: ✓ Connected
```

That's it — all 24 rUvOS tools are now available to Claude Code in every project.

> **What `setup.sh` removes:** only the incompatible **v2/v3 npm CLI**
> (`ruflo`, `@claude-flow/cli`) + its npm cache.
> **What it leaves alone (both coexist with rUvOS):**
> - `claude-flow` / `ruv-swarm` **MCP servers** — different tool namespace,
>   process, and data dir; no conflict. Name "rUvOS" in requests to disambiguate
>   overlapping capabilities.
> - Ruflo **Claude Code plugins** (the `ruflo` bundle → `ruflo-*` agents/skills) —
>   user-managed; remove via `/plugin` only if you want a fully ruflo-free setup.

**`setup.sh` flags:**

| Flag | Effect |
|------|--------|
| `--no-mcp` | Skip Claude Code MCP registration |
| `--prefix DIR` | Install the binary into `DIR` (default `/usr/local/bin`, else `~/.local/bin`) |
| `--help` | Show usage |

### Manual install (if you prefer)

```bash
cargo build --release
# Install onto your PATH. ~/.cargo/bin is already on PATH (you have Rust) — no sudo:
cp target/release/ruvos ~/.cargo/bin/ruvos               # or ~/.local/bin
export RUVOS_HOME="$HOME/.ruvos"                          # shared data dir (optional)
claude mcp add ruvos --scope user -- ruvos mcp serve      # register with Claude Code
claude mcp list                                           # ruvos: ✓ Connected
```

> For a **system-wide** install (all users), use `sudo cp target/release/ruvos
> /usr/local/bin/ruvos` instead — `sudo` is only needed there because
> `/usr/local/bin` is root-owned. A per-user dir like `~/.cargo/bin` needs none.

`RUVOS_HOME` defaults to `./.ruvos` in the current directory; set it to share one
memory/session store across every project.

---

## How you actually use it: just talk

**You do not type commands or keywords.** Once the MCP server is connected, Claude
Code sees the 24 tools and decides which to call, based on what you ask.

> 💡 **Say "rUvOS" in your request.** If you also have other agent MCP servers or
> plugins installed (e.g. legacy `ruflo` / `claude-flow`), several of them offer
> overlapping capabilities (memory, swarms, …). Naming rUvOS explicitly —
> *"use rUvOS to…"*, *"have rUvOS remember…"* — steers the request to rUvOS
> instead of leaving the choice to chance. The examples below all do this.

| You say… | …and rUvOS handles it with |
|----------|----------------------------|
| *"Use rUvOS to help me build a POST /users endpoint"* | `session.create`, `agent.spawn` |
| *"Have rUvOS remember we're using PostgreSQL for this project"* | `memory.store` |
| *"Ask rUvOS what we decided about the database schema"* | `memory.search` |
| *"Resume my rUvOS session from yesterday"* | `session.resume` |
| *"Have rUvOS orchestrate a full feature pipeline for user auth"* | `orchestrate.run` |
| *"Ask rUvOS if it's safe to run this command"* | `hooks.pre` (risk assessment) |
| *"Check rUvOS system health"* | `gov.health` |
| *"Show me the rUvOS audit log for the last hour"* | `gov.events` |

> These are **representative** mappings, not guarantees. *Which* tool Claude calls
> for a given sentence is its own runtime decision (model-dependent, not
> deterministic). Naming rUvOS makes it far more reliable, but the only 100%
> deterministic route is invoking the tool directly over MCP (see the feature
> reference below). What rUvOS guarantees is that the tools are **available** and
> **work** — every one is exercised by the test suite.

For a specific tool, just name it — e.g. *"have rUvOS fork this session before the
risky refactor"* → `session.fork`.

---

## The 24 tools

| Domain | Tools | What they do |
|--------|-------|--------------|
| **memory** (4) | `search`, `store`, `retrieve`, `list` | Persistent semantic memory — HNSW + RaBitQ vector search, MMR diversity, recency, and a temporal knowledge graph (`related_entities`) |
| **session** (3) | `create`, `resume`, `fork` | Resumable work sessions as **signed `.rvf` containers**; fork = copy-on-write branch with cryptographic lineage |
| **agent** (3) | `spawn`, `status`, `message` | Spawn/track/message agents across 12 archetypes; backed by the redb store + signed snapshots |
| **hooks** (3) | `pre`, `post`, `route` | Pre/post lifecycle hooks (incl. **safety risk assessment**) + model/archetype routing |
| **intel** (2) | `pattern_search`, `pattern_store` | SONA trajectory learning — store outcomes, retrieve similar past approaches |
| **plugin** (2) | `list`, `invoke` | Discover and run plugins (markdown + shell commands) |
| **gov** (3) | `health`, `witness_verify`, `events` | System health + safety score, `.rvf` signature verification, signed audit log |
| **relay** (3) | `announce`, `list`, `send` | Cross-instance coordination — independent Claude Code instances discover and message each other via pure file mailboxes (no daemon, no port, no DB) |
| **orchestrate** (1) | `run` | Orchestration templates: `feature` / `bugfix` / `refactor` / `security` |

---

## A natural-language session

In Claude Code you never type tool calls — you talk, and Claude calls the tools.
A typical session:

```
You:  Use rUvOS to build a POST /users endpoint with validation, and have it
      remember the design as we go.

Claude (routing to rUvOS because you named it):
  → session.create  { name: "users-endpoint" }
  → memory.store    { key: "spec", value: "POST /users, zod validation, ...",
                      namespace: "users-api" }
  → orchestrate.run { template: "feature", task: "POST /users with validation" }
  ...planner → coder → tester → reviewer run, each leaving a real artifact...

[next day]
You:  Resume my rUvOS session for the users endpoint.
Claude:
  → session.resume  { session_id: "..." }   # full context restored from signed .rvf
  → memory.search   { query: "users endpoint design", namespace: "users-api" }
```

Everything below shows the **same tools driven directly over MCP** — useful for
scripting, CI, tests, or any MCP client. rUvOS speaks JSON-RPC on stdin/stdout;
pipe one `initialize` line then `tools/call` lines into `ruvos mcp serve`.

---

## Feature reference — every tool, by example

Each tool below has a plain-English description, the phrase you'd typically say
(🗣️), and a small example showing the call and what it returns. To run any of
them yourself, wrap the call line with the transport boilerplate:

```bash
printf '%s\n' \
'{"jsonrpc":"2.0","id":0,"method":"initialize","params":{}}' \
'<the call line below>' \
| ruvos mcp serve
```

---

### `memory` — persistent semantic memory + knowledge graph

Vector search (HNSW + RaBitQ) with diversity and recency, plus a temporal
knowledge graph. Survives restarts.

**`memory.store`** — save a fact you want remembered later.
🗣️ *"rUvOS, Remember we're using PostgreSQL for this project."*
```jsonc
{"name":"memory.store","arguments":{"key":"db","value":"postgres connection pooling via pgbouncer","namespace":"proj","tags":["infra"]}}
// → { "status":"stored", "key":"db", "namespace":"proj" }
```

**`memory.search`** — recall by meaning, not exact words; also returns related
entities from the knowledge graph.
🗣️ *"rUvOS, What did we decide about the database?"*
```jsonc
{"name":"memory.search","arguments":{"query":"database connection","namespace":"proj","top_k":5}}
// → { "count":1, "results":[{ "key":"db", "value":"postgres connection pooling…", "score":0.64 }],
//     "related_entities":[{ "name":"Postgres", "summary":"…" }] }
```

**`memory.retrieve`** — fetch one entry by its exact key.
```jsonc
{"name":"memory.retrieve","arguments":{"key":"db","namespace":"proj"}}
// → { "found":true, "key":"db", "value":"postgres connection pooling…", "tags":["infra"] }
```

**`memory.list`** — list everything stored in a namespace.
```jsonc
{"name":"memory.list","arguments":{"namespace":"proj"}}
// → { "namespace":"proj", "count":1, "entries":[ … ] }
```

---

### `session` — resumable, signed work contexts

A session is a signed `.rvf` container on disk. You can pick work back up later,
and `fork` makes a copy-on-write branch with a cryptographic link to its parent.

**`session.create`** — start a session you can return to.
🗣️ *"rUvOS, Let's start working on the users endpoint."*
```jsonc
{"name":"session.create","arguments":{"name":"users-endpoint","state":{"branch":"feat/users"}}}
// → { "session_id":"6305…", "name":"users-endpoint", "rvf_path":".ruvos/rvf/6305….rvf", "status":"created" }
```

**`session.resume`** — restore the full context of a past session (the signature
is verified first).
🗣️ *"rUvOS, Pick up where we left off yesterday."*
```jsonc
{"name":"session.resume","arguments":{"session_id":"6305…"}}
// → { "found":true, "name":"users-endpoint", "state":{ "branch":"feat/users" }, "status":"resumed" }
```

**`session.fork`** — branch a session before a risky change; the child links
back to the parent.
🗣️ *"rUvOS, Fork this before we try the big refactor."*
```jsonc
{"name":"session.fork","arguments":{"source_session_id":"6305…"}}
// → { "forked_id":"a1b2…", "source_session_id":"6305…", "status":"forked", "success":true }
```

---

### `agent` — spawn, track, and message agents

Spawn one of 12 archetypes (coder, tester, reviewer, …). Each produces a real
work artifact on disk and is saved in the shared store.

**`agent.spawn`** — put an agent to work on a prompt.
🗣️ *"rUvOS, Get a coder to write the POST /users handler."*
```jsonc
{"name":"agent.spawn","arguments":{"archetype":"coder","prompt":"write the POST /users handler","model":"claude-haiku-4-5","traits":["backend"]}}
// → { "agent_id":"7ed0…", "archetype":"coder", "status":"completed",
//     "artifact_path":".ruvos/agents/7ed0…/output.md", "artifact_bytes":264 }
```

**`agent.status`** — see what agents exist and their state (all, or one by id).
🗣️ *"rUvOS, What are my agents up to?"*
```jsonc
{"name":"agent.status","arguments":{}}
// → { "count":2, "agents":[{ "agent_id":"7ed0…", "archetype":"coder", "status":"completed" }, … ] }
```

**`agent.message`** — send a follow-up message to an agent.
🗣️ *"rUvOS, Tell the coder to also add pagination."*
```jsonc
{"name":"agent.message","arguments":{"agent_id":"7ed0…","message":"also add pagination"}}
// → { "delivered":true, "message_id":"…", "message_count":1 }
```

---

### `hooks` — lifecycle hooks, safety & routing

Safety checks before risky actions, model/archetype routing, and outcome
recording that feeds learning.

**`hooks.pre`** — risk-assess an action before it runs; flags destructive
commands.
🗣️ *"rUvOS, Is it safe to run this command?"*
```jsonc
{"name":"hooks.pre","arguments":{"kind":"command","payload":{"command":"<a destructive shell command>"}}}
// → { "status":"ok", "blocked":true,
//     "safety":{ "passed":false, "safety_score":0.7,
//                "violations":[{ "constraint":"destructive_command", "level":"High" }] } }
```

**`hooks.route`** — pick the best archetype + model tier for a task.
🗣️ *"rUvOS, Who should handle a security audit?"*
```jsonc
{"name":"hooks.route","arguments":{"task":"audit auth flow for injection vulnerabilities"}}
// → { "archetype":"security", "model":"claude-opus-4-8", "tier":3, "confidence":0.8 }
```

**`hooks.post`** — record how an action turned out (feeds SONA learning).
```jsonc
{"name":"hooks.post","arguments":{"kind":"task","payload":{"task":"build endpoint"},"success":true,"message":"green"}}
// → { "status":"ok", … }
```

---

### `intel` — SONA trajectory learning

Remember the steps you took and how they turned out, then find similar past
approaches later.

**`intel.pattern_store`** — record a sequence of steps and its outcome.
🗣️ *"rUvOS, Remember how we did that migration."*
```jsonc
{"name":"intel.pattern_store","arguments":{"trajectory":["read schema","write migration","run tests"],"outcome":"success: migration applied"}}
// → { "status":"stored", "pattern_id":"…", "total_patterns":1 }
```

**`intel.pattern_search`** — find past approaches similar to what you're doing now.
🗣️ *"rUvOS, Have we done something like this before?"*
```jsonc
{"name":"intel.pattern_search","arguments":{"query":"database migration schema","top_k":5}}
// → { "count":1, "patterns":[{ "outcome":"success: migration applied", "score":0.71, … }] }
```

---

### `plugin` — discover and run plugins

Plugins are markdown + shell commands found under `./.ruvos/plugins`,
`~/.ruvos/plugins`, etc. `invoke` only runs commands a plugin actually declares
(command-injection guard).

**`plugin.list`** — see what plugins are installed.
🗣️ *"rUvOS, What plugins do I have?"*
```jsonc
{"name":"plugin.list","arguments":{}}
// → { "count":0, "plugins":[] }
```

**`plugin.invoke`** — run a command a plugin provides.
🗣️ *"rUvOS, Run my-plugin's build command."*
```jsonc
{"name":"plugin.invoke","arguments":{"plugin_name":"my-plugin","command":"build","args":["--release"]}}
// → { "status":0, "stdout":"…", "stderr":"" }   // unknown plugin → status:1 + reason in stderr
```

---

### `gov` — health, provenance & audit

**`gov.health`** — a real status report: tools, data dir, what's stored, safety score.
🗣️ *"rUvOS, What's the system health?"*
```jsonc
{"name":"gov.health","arguments":{}}
// → { "status":"ok", "version":"4.0.0-rc.1", "tool_count":24,
//     "persisted":{ "agents":2, "memory_entries":1, "sessions":1 },
//     "safety":{ "score":1.0, "active_constraints":5, "recent_violations":0 } }
```

**`gov.witness_verify`** — confirm a session file hasn't been tampered with.
🗣️ *"rUvOS, Is this .rvf file still valid?"*
```jsonc
{"name":"gov.witness_verify","arguments":{"rvf_path":".ruvos/rvf/6305….rvf"}}
// → { "rvf_path":"…", "verified":true, "exists":true }
```

**`gov.events`** — query the signed audit log of what happened.
🗣️ *"rUvOS, Show me what happened in the last hour."*
```jsonc
{"name":"gov.events","arguments":{"event_type":"agent.spawned","limit":20}}
// → { "count":2, "events":[{ "event_type":"agent.spawned", "agent_id":"7ed0…", "timestamp":… }, … ] }
```

---

### `relay` — cross-instance coordination

Two independent Claude Code instances (e.g. one on the backend, one on the
frontend) discover and message each other by sharing one `RUVOS_HOME`. **No
daemon, no port, no database** — presence and messages are plain files, delivered
the next time someone calls `relay.list`. Instances that go quiet for 60s are
pruned automatically.

```bash
# Both terminals point at the same relay directory:
export RUVOS_HOME=/home/you/.ruvos
```

**`relay.announce`** — tell other instances who you are and what you're doing.
🗣️ *"rUvOS, Let the other sessions know I'm on the backend."*
```jsonc
{"name":"relay.announce","arguments":{"summary":"backend: auth endpoints"}}
// → { "id":"A-uuid", "pid":…, "cwd":"…", "git_repo":"…", "summary":"backend: auth endpoints" }
```

**`relay.list`** — discover other live instances and read your own inbox (drained
on read). Scope is `machine`, `directory`, or `repo`.
🗣️ *"rUvOS, Who else is working right now, and any messages for me?"*
```jsonc
{"name":"relay.list","arguments":{"scope":"machine"}}
// → { "count":1, "relays":[{ "id":"A-uuid", "summary":"backend: auth endpoints" }],
//     "inbox":[{ "from":"B-uuid", "body":"login form posts to /auth/login — confirm the shape?" }] }
```

**`relay.send`** — message another instance by id.
🗣️ *"rUvOS, Ask the backend session to confirm the login shape."*
```jsonc
{"name":"relay.send","arguments":{"to":"A-uuid","body":"login form posts to /auth/login — confirm the shape?"}}
// → { "delivered":true, "message_id":"…" }
```

Every `announce`/`send` is recorded in the signed `gov.events` audit log.

---

### `orchestrate` — multi-agent orchestration templates

One call runs an ordered pipeline of agents; each step leaves a real artifact.
Templates: `feature` (planner → coder → tester → reviewer), `bugfix`
(researcher → coder → tester), `refactor` (architect → coder → reviewer),
`security` (security → coder → tester).

**`orchestrate.run`** — run a whole pipeline for a task in one go.
🗣️ *"rUvOS, Orchestrate a full feature pipeline for user auth."*
```jsonc
{"name":"orchestrate.run","arguments":{"template":"feature","task":"build POST /users with validation"}}
// → { "orchestration_id":"…", "template":"feature", "status":"completed", "step_count":4,
//     "steps":[ { "archetype":"planner",  "agent_id":"…", "artifact_path":"…" },
//               { "archetype":"coder",    … },
//               { "archetype":"tester",   … },
//               { "archetype":"reviewer", … } ] }
```

---

## Agent archetypes & traits

`agent.spawn` and `orchestrate.run` use 12 archetypes, composable with traits:

**Archetypes:** `coder`, `reviewer`, `tester`, `researcher`, `architect`, `planner`,
`security`, `perf`, `devops`, `data`, `docs`, `coordinator`

**Traits** (modify prompt + tool allow-list + model tier): `--trait=tdd`,
`--trait=backend`, `--trait=frontend`, `--trait=mobile`, `--trait=ml`,
`--trait=domain`, `--trait=cloud`, `--trait=db`, `--trait=audit`, and coordinator
`--topology=hierarchical|mesh|adaptive`.

---

## Where your data lives

All state persists under `$RUVOS_HOME` (default `./.ruvos`). Disk is the source of
truth — state survives restarts and is verifiable across processes.

```
$RUVOS_HOME/
├── rvf/<id>.rvf        # signed, witness-chained session containers
├── store.redb          # redb live store: agents, tasks, events, messages, metrics
├── memory.json         # memory entries (namespace → key → entry)
├── memory-graph.json   # temporal knowledge graph
├── intel.json          # SONA trajectory patterns
├── safety/safety.json  # safety constraints + violation log
├── agents/<id>/output.md   # real agent work artifacts
└── .rvf-key            # per-install signing key (0600; gitignored — never commit)
```

**Storage model:** `redb` (pure-Rust embedded DB) is the fast, queryable working
store; `.rvf` containers are signed, tamper-evident snapshots for provenance and
portability. No SQLite, no bundled C — the binary stays pure Rust.

---

## Architecture

```
crates/                    # rUvOS orchestration shell (the 6 new crates)
├── ruvos-cli              # clap CLI: `ruvos init`, `ruvos mcp serve`
├── ruvos-mcp              # JSON-RPC MCP server + the 24 tool handlers
├── ruvos-host             # CliHost trait + Claude/Codex adapters
├── ruvos-plugin-host      # plugin discovery + shell execution
├── ruvos-hooks            # 8 hooks + SONA learning (pure Rust, no SQLite)
└── ruvos-session          # .rvf containers + fork + witness-chain verify

substrate/                 # RuVector kernel + vendored capabilities (all pure Rust)
├── ruvector-core          # HNSW vector index + VectorDB (redb storage)
├── ruvector-rabitq        # 1-bit quantized ANN search
├── sona                   # self-optimizing pattern learning
├── rvf-crypto             # SHAKE-256 witness chains + Ed25519
├── ruvos-store            # redb store + signed .rvf snapshots
├── ruvos-memory-graph     # temporal knowledge graph (petgraph)
├── ruvos-safety           # behavioral guardrails / adaptive constraints
├── rulake                 # federated vector search over many backends
├── ruv-swarm-transport    # WebSocket + in-process agent messaging
└── … (rvf-* container stack, ruvector-math, mcp-brain)
```

**MCP protocol:** rUvOS implements the full handshake (`initialize` →
`notifications/initialized` → `tools/list` → `tools/call`), so any MCP client
(Claude Code, Codex CLI) discovers and calls the tools natively.

Key decisions are recorded as ADRs in `docs/spec/` (e.g. ADR-001: redb + `.rvf`
persistence and internal task ownership).

---

## Development

```bash
# Build / run
cargo build --release
ruvos mcp serve

# Full zero-defect gate (use --jobs 4 to avoid OOM on the 30+ crate tree)
cargo build  --workspace --jobs 4
cargo clippy --workspace --all-targets --jobs 4 -- -D warnings
cargo fmt --check
cargo test  --workspace --jobs 4
```

**Project rules** (enforced; see `CLAUDE.md`):
- **Zero-defect policy** — the entire workspace stays clean (0 errors, 0 warnings,
  0 failing tests) at all times, including vendored substrate crates.
- **File size limit** — every `.rs` file ≤ 500 lines.
- **One tool domain per scope** — new MCP tools require an ADR (current: 24 tools,
  budget 80).

---

## Acknowledgments

**rUvOS is built entirely on the foundational work of [rUv (Reuven Cohen /
@ruvnet)](https://github.com/ruvnet).**

rUv created the original ecosystem this project consolidates and re-implements in
Rust:

- **Ruflo / claude-flow** — the agent orchestration system rUvOS is the v4 rewrite of
- **RuVector** — the self-learning vector + graph + local-AI kernel (`ruvector-core`,
  `ruvector-rabitq`, `sona`, …)
- **The `.rvf` format & witness chains** (`rvf-crypto`, `rvf-*`) — signed, tamper-evident
  state containers
- **ruv-swarm / ruv-FANN** — swarm coordination, transport, and neural forecasting
- **RuLake**, **agentdb**, and the broader rUvnet research corpus

The architecture, the hard algorithms, and the original vision are rUv's. rUvOS's
contribution is a ruthless Rust-native consolidation — fewer tools, one static binary,
zero-defect discipline — on top of that foundation. **Thank you, rUv.** 🙏🚀

Explore the originals at **https://github.com/ruvnet**.

---

## License

MIT — consistent with the upstream rUvnet projects.
