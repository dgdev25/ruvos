<p align="center">
  <img src="assets/banner.svg" alt="rUvOS — gives your AI coding assistant a memory and a team. One static Rust binary, no Node." width="100%">
</p>

<p align="center">
  <img alt="version" src="https://img.shields.io/badge/version-4.0.0--rc.1-14b8a6">
  <img alt="built with Rust" src="https://img.shields.io/badge/built%20with-Rust-d9772a?logo=rust&logoColor=white">
  <img alt="protocol MCP" src="https://img.shields.io/badge/protocol-MCP-3b82f6">
  <img alt="60 MCP tools" src="https://img.shields.io/badge/MCP%20tools-60-2ac3de">
  <img alt="no Node" src="https://img.shields.io/badge/no%20Node-pure%20Rust-3fb950">
  <img alt="license MIT" src="https://img.shields.io/badge/license-MIT-blue">
</p>

## What is this?

Out of the box, an AI coding assistant forgets everything between sessions, works alone, and leaves no trace of *why* it did what it did. **rUvOS fixes that.**

It's a single small program you run once. After that, your assistant (Claude Code, Codex CLI, Gemini CLI) can **remember** decisions across days, **resume** exactly where it left off, **spin up a team of specialist agents**, **scan dependencies for known vulnerabilities**, **coordinate across terminals**, and keep a **signed, tamper-evident log** of everything — all stored on your own disk.

It connects through the **Model Context Protocol (MCP)** — the standard way tools talk to AI assistants — so you don't learn new commands. You just talk, and the assistant calls the right rUvOS tool for you.

It's **one static Rust binary**: no Node.js, no background service, no external database, no cloud account. (The offline CVE advisory database is SQLite via rusqlite-bundled — the only native dependency, compiled in.)

> **The tagline:** *RuVector is the kernel, rUvOS is the shell.* RuVector is the self-learning vector + graph + crypto substrate; rUvOS is the orchestration layer that turns it into tools your assistant can use.

> ### 🙏 Built on the work of [**rUv** (Reuven Cohen / @ruvnet)](https://github.com/ruvnet)
> rUvOS consolidates and re-implements rUv's ecosystem — **Ruflo / claude-flow**, **RuVector**, the **`.rvf`** format, **SONA**, **ruv-swarm**, and more — into one Rust binary. The hard, original ideas are his. **Huge thanks and full credit to rUv.** 🚀

> ⚠️ **Not compatible with the Ruflo v2/v3 npm CLI.** rUvOS v4 is a clean-room Rust rewrite; there's no migration path. Running [`./setup.sh`](#-quickstart) uninstalls that old npm CLI but **leaves your `claude-flow` / `ruv-swarm` MCP servers and plugins alone — they coexist fine** (separate namespaces, processes, and data dirs).

---

## ✨ Highlights

<p align="center">
  <img src="assets/features.svg" alt="Eleven tool domains: memory, session, agent, hooks, intel, plugin, gov, relay, orchestrate, swarm, compress." width="100%">
</p>

- 🧠 **Memory that lasts** — store facts and recall them by *meaning*. Hybrid search (dense HNSW + BM25 keywords) with a temporal knowledge graph and a feedback loop that learns which results were useful.
- 💾 **Resumable, signed sessions** — each work context is a signed `.rvf` file you can return to days later; `fork` branches one before a risky change with full cryptographic lineage.
- 👥 **A team of agents** — spawn 12 specialist archetypes (coder, tester, security, …); a GOAP A\* planner computes the pipeline for your goal, and failed steps retry or stop.
- 🔍 **CVE / vulnerability scanning** — scan any JS/TS or Rust project for known vulnerabilities via the OSV database; outputs JSON, SARIF 2.1.0 (GitHub Code Scanning), or a terminal table. Offline mode uses a local SQLite advisory database.
- 🛡️ **Safety + provenance built in** — risky actions are risk-checked before they run, and every action lands in a signed audit log you can verify.
- 📡 **Multi-terminal coordination** — independent Claude Code instances discover and message each other through plain files; no daemon, no port, no database. The optional `ruvos daemon watch` relay listener processes tasks dispatched from the relay bus and stores results back into memory.
- ⚡ **Agent execution bridge** (ADR-015) — `ruvos_agent_exec` closes the "markdown-only" gap: agents can now write files, run shell commands, and perform git operations directly, with optional OS-level sandbox isolation.
- 🗜️ **Context compression** — trim large JSON, log, code, and text payloads; originals are stored in signed session files when needed, with a replayable regression suite.
- 🔁 **Learning signals** — successful outcomes feed `memory` and `intent` stores so future routing improves without a second memory system.

---

## 🚀 Quickstart

<p align="center">
  <img src="assets/quickstart.svg" alt="Four steps: clone and run ./setup.sh, restart Claude Code, verify with claude mcp list showing ruvos Connected, then just talk." width="100%">
</p>

```bash
git clone https://github.com/dgdev25/ruvos.git
cd ruvos
./setup.sh
```

Then **restart Claude Code** (it loads MCP servers at startup), open a new terminal, and confirm:

```bash
claude mcp list           # ruvos: ✓ Connected
ruvos --version           # ruvos 4.0.0-rc.1
```

That's it — all 60 rUvOS tools are now available to Claude Code in every project.

<details>
<summary>What <code>setup.sh</code> does (10 steps, all idempotent)</summary>

1. Check prerequisites (`cargo`)
2. Build the release binary
3. Remove the incompatible v2/v3 npm CLI (`ruflo`, `@claude-flow/cli`)
4. Install `ruvos` binary → `/usr/local/bin` or `~/.local/bin`
5. Write `PATH` + `RUVOS_HOME` to your shell profile
6. Scaffold `~/.ruvos/{plugins,sessions,cve,agents,intel}`
7. Register rUvOS with Claude Code: `claude mcp add ruvos --scope user`
8. Wire Claude Code lifecycle hooks into `~/.claude/settings.json`:
   - `PreToolUse` → pre-hook (task/edit/command routing + safety)
   - `PostToolUse` → post-hook (SONA learning, trajectory store)
   - `Stop` → session checkpoint (`.rvf` fork)
9. Register rUvOS with Codex CLI (`~/.codex/config.json`) if installed
10. Smoke-test: binary version + MCP round-trip (expects ≥60 tools)

**Leaves alone:** `claude-flow` / `ruv-swarm` MCP servers and any Ruflo Claude Code plugins — they coexist fine (different namespaces, processes, data dirs).

**Flags:** `--no-mcp` (skip steps 7-9) · `--no-hooks` (skip step 8) · `--prefix DIR` (install location) · `--help`.
</details>

<details>
<summary>Manual install (if you prefer)</summary>

```bash
cargo build --release -p ruvos-cli
cp target/release/ruvos ~/.cargo/bin/ruvos
export RUVOS_HOME="$HOME/.ruvos"
claude mcp add ruvos --scope user -- ruvos mcp serve
claude mcp list   # ruvos: ✓ Connected
```

`RUVOS_HOME` defaults to `./.ruvos`; set it globally to share one memory/session store across every project.
</details>

---

## 🧭 How it works

**You don't type commands or keywords.** Once the MCP server is connected, Claude Code sees the 60 tools and decides which to call from what you ask. The loop:

<p align="center">
  <img src="assets/how-it-works.svg" alt="The loop: you ask in plain language → recall relevant past decisions → a planner computes the agent pipeline → agents run (failures retry or stop) → outcomes are learned, which sharpens the next recall and plan." width="100%">
</p>

You ask in plain language; rUvOS **recalls** relevant past decisions, a planner **computes** the agent pipeline for the goal, the **agents run** (a failed step retries or stops the pipeline), and the **outcome is learned** — sharpening the next recall and plan. Underneath every step, a safety gate vets risky actions and a signed audit log records what happened.

> 💡 **Say "rUvOS" in your request.** If you also run `claude-flow` or `ruv-swarm`, naming rUvOS explicitly — *"use rUvOS to…"*, *"have rUvOS remember…"* — reliably routes to it.

| You say… | …and rUvOS handles it with |
|----------|----------------------------|
| *"Use rUvOS to build a POST /users endpoint"* | `ruvos_session_create`, `ruvos_orchestrate_run` |
| *"Have rUvOS remember we're using PostgreSQL"* | `ruvos_memory_store` |
| *"Ask rUvOS what we decided about the schema"* | `ruvos_memory_search` |
| *"Resume my rUvOS session from yesterday"* | `ruvos_session_resume` |
| *"Scan this project for CVEs"* | `ruvos_gov_cve_lookup` |
| *"Ask rUvOS if it's safe to run this command"* | `ruvos_hooks_pre` (risk assessment) |
| *"Show me the rUvOS audit log for the last hour"* | `ruvos_gov_events` |
| *"Have rUvOS write this file and run the tests"* | `ruvos_agent_exec` (execution bridge) |

---

## 💡 A real session

```
You:  Use rUvOS to build a POST /users endpoint with validation, and remember
      the design as we go.

Claude:
  → ruvos_session_create    { name: "users-endpoint" }
  → ruvos_memory_store      { key: "spec", value: "POST /users, zod validation, …" }
  → ruvos_orchestrate_run   { template: "feature", task: "POST /users with validation" }
        planner → coder → tester → reviewer   (each leaves a real artifact)

[next day]
You:  Resume my rUvOS session for the users endpoint.
Claude:
  → ruvos_session_resume    { session_id: "…" }   # context restored from signed .rvf
  → ruvos_memory_search     { query: "users endpoint design" }
```

---

## 🔍 CVE Scanning

rUvOS ships a first-class vulnerability scanner for JS/TS and Rust projects (package-lock.json, pnpm-lock.yaml, yarn.lock, Cargo.lock) — callable from Claude Code via the `ruvos_gov_cve_lookup` MCP tool, or directly from the CLI:

```bash
# Terminal output (default — sorted by severity)
ruvos cve scan /path/to/project

# JSON — machine-readable full ScanResult
ruvos cve scan --json /path/to/project

# SARIF 2.1.0 — upload to GitHub Code Scanning
ruvos cve scan --sarif /path/to/project > results.sarif

# CI gate — exit non-zero if any High+ vuln found
ruvos cve scan --fail-on high /path/to/project

# Prod only, offline, minimum severity threshold
ruvos cve scan --prod-only --offline --min-severity medium /path/to/project
```

**Supported lockfiles:** `package-lock.json` (npm v1/v2/v3), `npm-shrinkwrap.json`, `pnpm-lock.yaml` (v5/v6/v9), `yarn.lock` (v1 + Berry/v2+), `Cargo.lock` (crates.io ecosystem; direct deps classified from `Cargo.toml`).

**Data sources:**
- Online: [OSV API](https://osv.dev) batch queries, results cached at `$RUVOS_HOME/cve/osv-cache.json` (30-min TTL)
- Offline: SQLite advisory database (`--offline-db <path>`) compatible with the cve-lite-cli schema; semver range matching (introduced ≤ version < fixed)

**All flags:** `--json` · `--sarif` · `--prod-only` · `--offline` · `--offline-db <PATH>` · `--min-severity {low,medium,high,critical}` · `--fail-on {low,medium,high,critical}` · `--no-cache`

---

## 📚 Complete Feature Reference

Two surfaces: **CLI commands** you run in a terminal, and **MCP tools** Claude Code calls for you once the server is connected. Both drive the same persistent state under `$RUVOS_HOME` (default `./.ruvos`).

### Part 1 — CLI commands (`ruvos <command>`)

Run `ruvos <command> --help` for the full flag list of any command.

| Command | What it does | Example |
|---------|--------------|---------|
| `init` | Create/update `CLAUDE.md` with the ruvos managed block + the `.ruvos/` data dir | `ruvos init --name my-project` |
| `init --hooks` | Also write `.claude/settings.json` hook bindings so the 8 hooks fire mechanically (ADR-038) | `ruvos init --hooks` |
| `mcp serve` | Start the JSON-RPC MCP server on stdio (this is what Claude Code connects to) | `claude mcp add ruvos -- ruvos mcp serve` |
| `status` | Read-only live system view: health, swarm, agents, events, relays (ADR-039) | `ruvos status` · `ruvos status --json` |
| `cve scan` | Scan a project's lockfiles for vulnerable deps via OSV/CVE | `ruvos cve scan --fail-on high .` |
| `hook <kind>` | Dispatch a lifecycle hook event from the harness (reads JSON on stdin) | `echo '{}' \| ruvos hook edit --phase pre` |
| `plugin install` | Fetch, checksum-verify, and unpack a plugin tarball (ADR-040) | `ruvos plugin install demo --from ./demo.tar.gz` |
| `compress` | Compress stdin/a file using the frozen baseline algorithm | `cat big.log \| ruvos compress --kind log` |
| `contracts` | Generate or verify the canonical tool/archetype/hook manifest | `ruvos contracts check docs/contracts/contract-manifest.json` |
| `doctor` | Local health/invariant check (substrate, persisted counts, safety score) | `ruvos doctor --strict` |
| `eval` | Run the compression regression suite with optional baselines | `ruvos eval compress --compare-to reports/baseline.json` |
| `skills` | Audit a skills corpus and build a portable redb skills pack | `ruvos skills audit --corpus-root <path>` |
| `daemon watch` | Persistent relay-inbox listener that dispatches tasks to `agent_exec` | `ruvos daemon watch --poll-ms 100` |

### Part 2 — MCP tools (60 tools, called by Claude Code)

In normal use you never type these — you ask in plain language and Claude Code picks the tool. To call one directly for testing, wrap it with the transport boilerplate:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '<the call line below>' \
| ruvos mcp serve
```

> **Tool names are `ruvos_<domain>_<action>`** (e.g. `ruvos_memory_store`). Copy them verbatim — there is no dot-notation alias.

### `memory` (4) — persistent semantic memory + knowledge graph

Hybrid retrieval (dense HNSW + BM25, fused), MMR diversity, recency weighting, temporal knowledge graph, and a feedback loop. Survives restarts (persisted to `$RUVOS_HOME/memory.json`).

| Tool | What it does | Example arguments |
|------|--------------|-------------------|
| `ruvos_memory_store` | Insert/update an entry with tags | `{"key":"db","value":"postgres via pgbouncer","namespace":"proj","tags":["infra"]}` |
| `ruvos_memory_search` | Hybrid semantic + lexical search (optional `filter_tags`) | `{"query":"database connection","namespace":"proj","top_k":5}` |
| `ruvos_memory_retrieve` | Fetch one entry by key | `{"key":"db","namespace":"proj"}` |
| `ruvos_memory_list` | List a namespace with filters | `{"namespace":"proj"}` |

```jsonc
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ruvos_memory_store","arguments":{"key":"db","value":"postgres pooling via pgbouncer","namespace":"proj","tags":["infra"]}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ruvos_memory_search","arguments":{"query":"database connection","namespace":"proj","top_k":5}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ruvos_memory_retrieve","arguments":{"key":"db","namespace":"proj"}}}
```

### `session` (3) — resumable, signed work contexts

Sessions are signed `.rvf` containers (HMAC-SHA256 + SHAKE-256 witness chain). Forking creates a COW branch with cryptographic lineage proof.

| Tool | What it does | Example arguments |
|------|--------------|-------------------|
| `ruvos_session_create` | Start a session, persist as `.rvf` | `{"name":"users-endpoint","state":{"branch":"feat/users"}}` |
| `ruvos_session_resume` | Restore a session by id | `{"session_id":"6305…"}` |
| `ruvos_session_fork` | COW-branch a session for parallel work | `{"source_session_id":"6305…"}` |

```jsonc
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ruvos_session_create","arguments":{"name":"users-endpoint","state":{"branch":"feat/users"}}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ruvos_session_resume","arguments":{"session_id":"6305…"}}}
```

### `agent` (4) — spawn, track, message, and execute

**12 archetypes:** `coder` · `reviewer` · `tester` · `researcher` · `architect` · `planner` · `security` · `perf` · `devops` · `data` · `docs` · `coordinator` — **9 composable traits:** `backend` · `frontend` · `mobile` · `cloud` · `db` · `ml` · `tdd` · `domain` · `audit`

| Tool | What it does | Example arguments |
|------|--------------|-------------------|
| `ruvos_agent_spawn` | Spawn a host agent | `{"archetype":"coder","prompt":"write POST /users","model":"claude-haiku-4-5","traits":["backend"]}` |
| `ruvos_agent_status` | List running agents + states | `{}` |
| `ruvos_agent_message` | Send a message to a named agent | `{"agent_id":"7ed0…","message":"also add pagination"}` |
| `ruvos_agent_exec` | Execution bridge — write files, run commands, git ops (see below) | `{"ops":[{"op":"run_command","cmd":"cargo","args":["test"]}]}` |

```jsonc
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ruvos_agent_spawn","arguments":{"archetype":"coder","prompt":"write POST /users","model":"claude-haiku-4-5","traits":["backend"]}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ruvos_agent_status","arguments":{}}}
```

### `hooks` (3) — safety, routing, learning

| Tool | What it does | Example arguments |
|------|--------------|-------------------|
| `ruvos_hooks_pre` | Pre-action risk assessment (`task`/`edit`/`command`) | `{"kind":"command","payload":{"command":"rm -rf /important"}}` |
| `ruvos_hooks_route` | Recommend archetype + model tier for a task | `{"task":"audit auth for injection"}` |
| `ruvos_hooks_post` | Record outcome → SONA learning | `{"kind":"task","payload":{},"success":true}` |

```jsonc
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ruvos_hooks_pre","arguments":{"kind":"command","payload":{"command":"rm -rf /important"}}}}
// → { "blocked":true, "safety":{ "passed":false, "violations":[…] } }
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ruvos_hooks_route","arguments":{"task":"audit auth for injection"}}}
// → { "archetype":"security", "model":"claude-opus-4-8", "tier":3, "confidence":0.91 }
```

### `intel` (5) — SONA trajectory + intent learning

| Tool | What it does | Example arguments |
|------|--------------|-------------------|
| `ruvos_intel_pattern_store` | Store a trajectory + outcome | `{"trajectory":["read schema","write migration"],"outcome":"success"}` |
| `ruvos_intel_pattern_search` | Find similar past trajectories | `{"query":"database migration","top_k":5}` |
| `ruvos_intel_intent_store` | Persist a durable goal/preference | `{"kind":"goal","text":"always use TS strict mode","tags":["project-default"]}` |
| `ruvos_intel_intent_search` | Search durable goals/preferences | `{"query":"typescript","kind":"goal"}` |
| `ruvos_intel_repo_inspect` | Snapshot repo health: hotspots, test gaps | `{"path":"."}` |

```jsonc
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ruvos_intel_pattern_store","arguments":{"trajectory":["read schema","write migration"],"outcome":"success"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ruvos_intel_repo_inspect","arguments":{"path":"."}}}
```

### `plugin` (2) — discover and run plugins

| Tool | What it does | Example arguments |
|------|--------------|-------------------|
| `ruvos_plugin_list` | Installed plugins + skills (discovered from disk) | `{}` |
| `ruvos_plugin_invoke` | Run a plugin command via its frontmatter `exec` entrypoint | `{"plugin_name":"my-plugin","command":"build","args":["--release"]}` |

```jsonc
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ruvos_plugin_list","arguments":{}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ruvos_plugin_invoke","arguments":{"plugin_name":"my-plugin","command":"build","args":["--release"]}}}
```

**Plugin layout** (drop into `.ruvos/plugins/<name>/` or `~/.ruvos/plugins/<name>/`):
```
<name>/
├── plugin.toml        # name, version, description
├── agents/*.md        # Claude Code agent definitions (YAML frontmatter)
├── skills/*/SKILL.md  # Claude Code skills
└── commands/*.md      # slash commands
```

**Installing plugins** ([ADR-040](docs/spec/adr-040-plugin-install.md)) — fetch, verify, and unpack a plugin tarball in one step:

```bash
ruvos plugin install demo --from https://example.com/demo.tar.gz   # or a local path
```

A `.sha256` sidecar next to the tarball is **required**; an HMAC-SHA256 `.sig` sidecar is verified when `RUVOS_PLUGIN_KEY` is set. Publishing convention:

```bash
tar -czf demo.tar.gz -C plugin-dir . && sha256sum demo.tar.gz > demo.tar.gz.sha256
```

### `gov` (12) — health, CVE scanning, audit, swarm governance

| Tool | What it does | Example arguments |
|------|--------------|-------------------|
| `ruvos_gov_health` | Doctor/status across substrate, hosts, MCP, daemon | `{}` |
| `ruvos_gov_cve_lookup` | Scan a project for vulnerable deps via OSV/CVE | `{"project_path":"/path","format":"json","min_severity":"high"}` |
| `ruvos_gov_witness_verify` | Verify a `.rvf` signature chain | `{"rvf_path":".ruvos/rvf/6305….rvf"}` |
| `ruvos_gov_events` | Query the signed audit/event log | `{"event_type":"agent.spawned","limit":20}` |
| `ruvos_gov_replay` | Replay a session/task trace from events | `{"session_id":"6305…"}` |
| `ruvos_gov_report` | Governance report with quality/benchmark signals | `{}` |
| `ruvos_gov_sprint_summary` | Aggregate sprint metrics from swarm + events | `{"sprint_id":"sprint-7"}` |
| `ruvos_gov_swarm_recommendation` | Recommend swarm topology for a task | `{"objective":"ship auth","members":[{"agent_id":"w1","role":"coder"}]}` |
| `ruvos_gov_swarm_plan` | Concrete swarm role/phase plan | `{"objective":"ship auth","members":[…]}` |
| `ruvos_gov_swarm_status` | Active swarm summary + suggested plan | `{}` |
| `ruvos_gov_swarm_policy` | Inspect learned swarm policy entries | `{}` |
| `ruvos_gov_swarm_history` | Recent swarm runs + learning outcomes | `{"limit":5}` |

```jsonc
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ruvos_gov_health","arguments":{}}}
// → { "status":"ok", "tool_count":60, "persisted":{…}, "safety":{"score":1.0} }
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ruvos_gov_cve_lookup","arguments":{"project_path":"/path/to/project","format":"json","min_severity":"high"}}}
// → { "status":"clean"|"vulnerable", "finding_count":N, "highest_severity":"…", "fix_count":N }
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ruvos_gov_events","arguments":{"event_type":"agent.spawned","limit":20}}}
```

### `gov` issues (6) — built-in issue tracker (beads_rust)

A lightweight issue tracker persisted in `$RUVOS_HOME`, with dependency edges between issues.

| Tool | What it does | Example arguments |
|------|--------------|-------------------|
| `ruvos_gov_issue_create` | Create an issue | `{"title":"flaky parallel tests","priority":"high"}` |
| `ruvos_gov_issue_list` | List issues with status/priority filters | `{"status":"open","priority":"high"}` |
| `ruvos_gov_issue_show` | Full issue details + comment history | `{"id":"bd-12"}` |
| `ruvos_gov_issue_close` | Close an issue with an optional reason | `{"id":"bd-12","reason":"fixed in c95f3a6"}` |
| `ruvos_gov_issue_search` | Full-text search across issues | `{"query":"ENOENT"}` |
| `ruvos_gov_issue_dep` | Add a dependency between two issues | `{"from":"bd-12","to":"bd-9"}` |

```jsonc
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ruvos_gov_issue_create","arguments":{"title":"flaky parallel tests","priority":"high"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ruvos_gov_issue_list","arguments":{"status":"open"}}}
```

### `relay` (6) — cross-instance coordination

Two rUvOS instances sharing one `RUVOS_HOME` discover and message each other via plain JSON file mailboxes (no daemon, no port). Every coordination action is recorded in the signed audit log.

```jsonc
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ruvos_relay_announce","arguments":{"summary":"backend: auth endpoints"}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ruvos_relay_list","arguments":{"scope":"machine"}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ruvos_relay_send","arguments":{"to":"ruvos-daemon","body":"{\"method\":\"exec\",\"correlation_id\":\"t1\",\"params\":{\"ops\":[{\"op\":\"run_command\",\"cmd\":\"npm\",\"args\":[\"test\"]}]}}"}}}
```

**Named-agent presence** — `relay::announce_as(id, summary)` writes a presence file under a stable name (e.g. `ruvos-daemon`) instead of the ephemeral process UUID, so `relay_send` can resolve it by name across sessions.

### `agent_exec` — execution bridge (ADR-015)

`ruvos_agent_exec` closes the agent "markdown-only" gap: agents can now write files, run shell commands, perform git operations, and optionally isolate all work in a fresh OS-level temp directory.

```jsonc
// write a file, run tests, commit — all in one call
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ruvos_agent_exec","arguments":{
  "ops":[
    {"op":"write_file","path":"src/lib.rs","content":"…"},
    {"op":"run_command","cmd":"cargo","args":["test"]},
    {"op":"git_op","git_op":"commit","message":"feat: add impl"}
  ]
}}}

// sandbox mode — all paths relative to a fresh temp dir (nothing touches the host tree)
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ruvos_agent_exec","arguments":{
  "sandbox":true,
  "ops":[{"op":"run_command","cmd":"make","args":["build"]}]
}}}
```

**Ops:** `write_file` · `read_file` · `run_command` (with optional `cwd`) · `git_op` (`add` · `commit` · `status` · `diff`). Pipeline stops on the first non-zero exit code.

### `ruvos daemon watch` — relay inbox listener

A persistent background process that polls the relay bus and dispatches tasks to `ruvos_agent_exec`. Results are stored in `memory` namespace `daemon` so any instance can read them.

```bash
ruvos daemon watch                        # listens on "ruvos-daemon" inbox
ruvos daemon watch --agent-id my-agent    # custom inbox name
ruvos daemon watch --poll-ms 100          # faster polling
```

Send tasks from any Claude Code session:

```jsonc
// fire-and-forget exec via relay
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ruvos_relay_send","arguments":{
  "to":"ruvos-daemon",
  "body":"{\"method\":\"exec\",\"correlation_id\":\"build-1\",\"params\":{\"ops\":[{\"op\":\"run_command\",\"cmd\":\"cargo\",\"args\":[\"build\"]}]}}"
}}}

// read result from memory once the daemon has processed it
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ruvos_memory_retrieve","arguments":{"key":"daemon/results/build-1","namespace":"daemon"}}}
```

### `ruvos status` — live system view

A read-only, human-facing snapshot of everything the MCP tools know: health, active swarm + members, agents, recent events, relay instances. Pure presentation over the same handlers the MCP tools use, so the CLI and MCP can never disagree (ADR-039). Add `--json` for the raw merged JSON.

```bash
ruvos status          # human view
ruvos status --json   # raw merged JSON for scripting
```

```text
rUvOS system status

── Health ──────────────────────────────
  status: ok  version: 4.0.0-rc.1  pid: 12345  tools: 60
  data root: /home/you/.ruvos
  persisted: 2 session(s), 7 memory entr(ies), 1 agent(s), 0 intel pattern(s)

── Swarm ──────────────────────────────
  no active swarm

── Agents ──────────────────────────────
  none

── Recent events ──────────────────────────────
  2026-06-12T09:09:38Z  agent.status.listed  agent: -

── Relay instances ──────────────────────────────
  none
```

### `orchestrate` — planned multi-agent pipelines

A GOAP (A\*) planner computes the archetype sequence from a template or a goal + capabilities. Optional `max_retries` loops a failed step back for bounded rework. **Templates:** `feature` · `bugfix` · `refactor` · `security` · `sparc`.

| Tool | What it does | Example arguments |
|------|--------------|-------------------|
| `ruvos_orchestrate_run` | Run a planned multi-agent pipeline (template- or goal-driven) | `{"template":"feature","task":"build POST /users"}` |

```jsonc
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ruvos_orchestrate_run","arguments":{"template":"feature","task":"build POST /users"}}}
// → { "status":"completed", "planned":true, "plan_cost":4.0, "steps":["planner","coder","tester","reviewer"] }
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ruvos_orchestrate_run","arguments":{"task":"harden auth","goal":{"secured":true,"tested":true}}}}
// → { "template":"custom", "planned":true, "steps":["security","coder","tester"] }
```

### `compress` (1) — context compression

Trims large JSON, log, code, and text payloads before they re-enter context. When `session_id` is provided, the original is stored in the signed `.rvf` session; the returned `original_ref` is later retrievable via `{"retrieve_ref":"<ref>"}`.

| Tool | What it does | Example arguments |
|------|--------------|-------------------|
| `ruvos_compress_run` | Compress text/JSON/code/logs, return a retrieval ref | `{"content":"…large payload…","kind":"auto","session_id":"6305…"}` |

```jsonc
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ruvos_compress_run","arguments":{"content":"…large payload…","kind":"auto","session_id":"6305…"}}}
// retrieve the original later
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ruvos_compress_run","arguments":{"retrieve_ref":"a1b2c3…"}}}
```

Regression testing from the CLI: `ruvos eval compress [--write reports/baseline.json | --compare-to reports/baseline.json]`.

### `swarm` (13) — multi-agent topology lifecycle

Durable swarms with topology (`hierarchical`/`mesh`/`hybrid`/`adaptive`), members, heartbeats, and learned policy. State is cross-process locked and atomically persisted.

| Tool | What it does | Example arguments |
|------|--------------|-------------------|
| `ruvos_swarm_create` | Create a swarm with topology, roles, objective | `{"objective":"ship auth","topology":"hierarchical","members":[{"agent_id":"w1","role":"coder"}]}` |
| `ruvos_swarm_status` | Inspect membership and progress | `{}` |
| `ruvos_swarm_assign` | Assign a task to a member | `{"agent_id":"w1","task_id":"t1"}` |
| `ruvos_swarm_heartbeat` | Refresh a member's liveness | `{"agent_id":"w1"}` |
| `ruvos_swarm_message` | Message a member or broadcast | `{"to":"w1","body":"ping"}` |
| `ruvos_swarm_complete` | Mark the swarm completed | `{"summary":"shipped"}` |
| `ruvos_swarm_fail` | Mark the swarm failed | `{"reason":"blocked"}` |
| `ruvos_swarm_health` | Liveness, utilization, freshness | `{}` |
| `ruvos_swarm_rebalance` | Move tasks off stale members | `{}` |
| `ruvos_swarm_join` | Add/reactivate a member | `{"agent_id":"w2"}` |
| `ruvos_swarm_leave` | Mark a member as left | `{"agent_id":"w1","force":true}` |
| `ruvos_swarm_report` | Summary with recent activity | `{}` |
| `ruvos_swarm_metrics` | Numeric health + throughput metrics | `{}` |

```jsonc
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"ruvos_swarm_create","arguments":{"objective":"ship auth","topology":"hierarchical","members":[{"agent_id":"w1","role":"coder"}]}}}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ruvos_swarm_assign","arguments":{"agent_id":"w1","task_id":"t1"}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ruvos_swarm_status","arguments":{}}}
```

---

## 🏗️ Architecture

rUvOS is two layers in one binary: a thin **orchestration shell** (8 Rust crates, ~24k LOC) on top of the **RuVector kernel + substrate** (pure-Rust vector search, learning, graph, crypto, planning, and coordination).

<p align="center">
  <img src="assets/architecture.svg" alt="Two layers. Top: rUvOS orchestration shell — ruvos-cli, ruvos-mcp (60 tools), ruvos-host (CLI adapters), plugin-host, ruvos-hooks, ruvos-session, ruvos-cve-lite, ruvos-compress. Bottom: RuVector substrate — HNSW/RaBitQ/ACORN, SONA, knowledge graph, GOAP A*, DAG retry, redb, .rvf witness, safety, DTW stream analysis, swarm transport." width="100%">
</p>

**Disk is the source of truth.** All state persists under `$RUVOS_HOME` (default `./.ruvos`) — `redb` is the fast working store, `.rvf` containers are signed tamper-evident snapshots, and `memory.json` / `intel.json` / `agents.json` are the durable JSON stores readable across processes.

<details>
<summary>Where your data lives</summary>

```
$RUVOS_HOME/
├── rvf/<id>.rvf           # signed, witness-chained session containers
├── store.redb             # redb live store: agents, tasks, events, messages, metrics
├── memory.json            # memory entries (namespace → key → entry)
├── memory-graph.json      # temporal entity co-occurrence graph
├── intel.json             # SONA trajectory patterns
├── intent.json            # stable preferences and goals
├── agents.json            # agent registry
├── agents/<id>/output.md  # agent work artifacts
├── swarm.json             # swarm topology + policy
├── cve/
│   └── osv-cache.json     # OSV query cache (30-min TTL)
└── .rvf-key               # per-install signing key (0600; gitignored — never commit)
```
</details>

### The 8 rUvOS crates

| Crate | LOC | Purpose |
|-------|-----|---------|
| `ruvos-cli` | 2,472 | clap-based binary: 13 subcommands |
| `ruvos-mcp` | 16,269 | JSON-RPC 2.0 MCP server + 60 tool handlers |
| `ruvos-host` | 415 | `CliHost` trait + Claude Code / Codex CLI adapters |
| `ruvos-plugin-host` | 565 | Plugin discovery (markdown + TOML), shell exec |
| `ruvos-hooks` | 373 | 8 hook kinds + SONA bridge |
| `ruvos-session` | 670 | `.rvf` containers, fork (COW), HMAC witness chain |
| `ruvos-cve-lite` | 2,032 | CVE/OSV scanner: parsers, client, cache, offline DB |
| `ruvos-compress` | 1,261 | Content compression + regression eval |

Plus **15 RuVector substrate crates** (HNSW, SONA, GOAP, redb store, `.rvf` crypto, RuLake, swarm transport, memory graph, skills pack, safety, and more) — 23 workspace members total, all building cleanly with zero warnings. (Substrate crates with no member consumers were dropped from the build; their sources remain in `substrate/`.)

### MCP transport

- **Protocol:** JSON-RPC 2.0 (custom implementation, ~500 LOC)
- **I/O:** tokio stdin/stdout
- **Handshake:** `initialize` → `notifications/initialized` → `tools/list` → `tools/call`
- **Stateless server:** No in-memory state; all persistence is disk-backed — process restarts are transparent

### Session + signing

`.rvf` containers carry a payload (JSON state) plus a `witness` object: an HMAC-SHA256 signature over the JSON bytes. Forking extends the parent's witness chain with the new action hash, producing a cryptographic lineage proof. Signing key lives at `$RUVOS_HOME/.rvf-key` (generated on first use; never committed).

### Hook kinds (8 total)

`task.pre` · `task.post` · `edit.pre` · `edit.post` · `command.pre` · `command.post` · `session.pre` · `session.post`

---

## 🩺 Status

**`v4.0.0-rc.1` — production-grade.** 60 MCP tools, 455 tests passing in ruvos-mcp, zero compiler/clippy warnings across the entire 23-crate workspace, and a deterministically green `cargo test --workspace` (standing zero-defect policy).

**Honest scope notes:**
- Vector ranking uses TF cosine similarity + HNSW + RuLake (real, working algorithms); neural embeddings are feature-hashing today — a provider API can be swapped in behind the same interface.
- `.rvf` signing is HMAC-SHA256 + SHAKE-256 witness chains (real and verified); the full distributed witness-chain federation is deferred to v2.
- The agent **runner** is optional; without `RUVOS_AGENT_RUNNER` set, agents produce real artifacts and report success by default.
- Gemini CLI adapter: architecture ready (same `CliHost` trait), implementation deferred.
- Local LLM inference (`ruvllm`): excluded from workspace (pulls candle/hf-hub), deferred to v2 per the no-local-inference decision.

---

## 🛠️ Development

```bash
# Build
cargo build --workspace --jobs 4

# Zero-defect gate (full workspace — use --jobs 4 to avoid OOM on 30+ crates)
cargo build --workspace --jobs 4
cargo clippy --workspace --all-targets --jobs 4 -- -D warnings
cargo test --workspace --jobs 4
cargo fmt --check

# Or via just
just build
just clippy
just test
just fmt
just doctor           # ruvos doctor --strict
just contracts-check  # validate contract-manifest.json
```

**Enforced rules** (see `CLAUDE.md`): zero-defect workspace at all times including vendored substrate; every `.rs` file ≤ 500 lines; new MCP tools require an ADR explaining which tool they replace or what domain gap they fill.

Architecture decisions are recorded as ADRs in [`docs/spec/`](docs/spec). The live tool/archetype/hook contract is generated into [`docs/contracts/contract-manifest.json`](docs/contracts/contract-manifest.json) and verified by `just contracts-check`.

---

## 📄 License

MIT — consistent with the upstream rUvnet projects. **Thank you, [rUv](https://github.com/ruvnet).** 🙏
