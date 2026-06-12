# rUvOS — Phase 0 Scope Ledger

> The contract that gates the Rust rewrite. Everything not on this list is **deleted**, not deferred. If a tool / agent / hook is missing from this document and someone wants it back, it goes through a new ADR — not a "while we're at it" port.
>
> Read alongside `REWRITE-SUMMARY.md`. This is the artifact Phase 0 must produce; Phase 1 cannot start until it's signed off.
>
> Live implementation note: the machine-readable contract source of truth is
> [`docs/contracts/contract-manifest.json`](../contracts/contract-manifest.json).
> If that file disagrees with this ledger, the manifest and implementation win.

---

## 0. Naming decisions to confirm before signing off

| Decision | Default in this ledger |
|---|---|
| Project / repo | **rUvOS** |
| Published binary | **`ruflo`** (preserves `npx ruflo` muscle memory) |
| Crate families | **`ruvector-*`** (existing substrate) + **`ruflo-*`** (new orchestration) |
| MCP server registration | `claude mcp add ruflo -- ruflo mcp serve` |
| Tagline | "The agentic operating system. RuVector is its kernel, Ruflo is its shell." |

---

## 1. The 20 v1 MCP tools

Naming: **`<domain>.<verb>`** (dotted, lowercase). One namespace per domain. No legacy aliases — `pre-bash`, `route-task`, `post-bash`, etc. are deleted, not aliased.

### memory — 4 tools

| # | Tool | Purpose | Backed by |
|---|---|---|---|
| 1 | `memory.search` | Semantic search across namespaces with MMR diversity + recency weighting | `ruvector-core` HNSW + `sona` reranker |
| 2 | `memory.store` | Insert/update an entry with optional embedding + tags | `ruvector-core` |
| 3 | `memory.retrieve` | Get a single entry by key | `ruvector-core` |
| 4 | `memory.list` | List entries in a namespace with filters | `ruvector-core` |

### session — 3 tools

| # | Tool | Purpose | Backed by |
|---|---|---|---|
| 5 | `session.create` | Start a session, return id, persist as `.rvf` | `rvf` |
| 6 | `session.resume` | Restore a session by id (full context + memory) | `rvf` + `ruvector-core` |
| 7 | `session.fork` | COW-branch a session for parallel exploration | `rvf-cow` |

### agent — 3 tools

| # | Tool | Purpose | Backed by |
|---|---|---|---|
| 8 | `agent.spawn` | Spawn a host agent: `{host, archetype, prompt, traits, model, budget}` | `ruflo-host` |
| 9 | `agent.status` | List running agents + states | `ruflo-host` |
| 10 | `agent.message` | Send message to a named agent (the `SendMessage` pattern that survives) | `ruflo-host` |

### hooks — 3 tools

| # | Tool | Purpose | Backed by |
|---|---|---|---|
| 11 | `hooks.pre` | Unified pre-hook (`{kind: "task"\|"edit"\|"command", payload}`) — returns routing + context | `ruflo-hooks` |
| 12 | `hooks.post` | Unified post-hook with outcome — feeds SONA learning | `ruflo-hooks` + `sona` |
| 13 | `hooks.route` | Get model + archetype recommendation for a task | `ruvector-router-core` |

### intel — 2 tools

| # | Tool | Purpose | Backed by |
|---|---|---|---|
| 14 | `intel.pattern_search` | Find similar past trajectories (4-step retrieve phase) | `sona` + `ruvector-core` |
| 15 | `intel.pattern_store` | Store outcome for the distill/consolidate phases | `sona` |

### plugin — 2 tools

| # | Tool | Purpose | Backed by |
|---|---|---|---|
| 16 | `plugin.list` | Installed plugins + skills (discovered from disk) | `ruflo-plugin-host` |
| 17 | `plugin.invoke` | Run a plugin command (shell exec via tokio) | `ruflo-plugin-host` |

### gov — 2 tools

| # | Tool | Purpose | Backed by |
|---|---|---|---|
| 18 | `gov.witness_verify` | Verify `.rvf` signature chain (replaces witness manifest #2047) | `rvf-crypto` |
| 19 | `gov.health` | Doctor / status across substrate, hosts, MCP, daemon | `ruflo-cli` |

### workflow — 1 tool

| # | Tool | Purpose | Backed by |
|---|---|---|---|
| 20 | `workflow.run` | Execute an orchestration template (`feature` / `bugfix` / `refactor` / `security`) — tri-mode `<host>:<archetype>:<task>` | `ruflo-host` + `ruflo-hooks` |

**Total: 20 tools (v1 baseline).** Stopping budget: 80 ever. Any new tool requires an ADR documenting which existing tool it's replacing or what domain gap it fills.

> **Live registry (ADR-037):** the list above is the historical v1 baseline. The binding, machine-checked source of truth for the *current* tool surface is `docs/contracts/contract-manifest.json` (60 tools as of ADR-036, each added via ADR-001…036; regenerate with `just contracts-generate`, verify with `just contracts-check`).

### What's deleted vs the current 323

| Category | Cut | Reason |
|---|---|---|
| All `*-bash` / `pre-bash` / `post-bash` aliases | yes | v2 compat — no legacy needed |
| All `hive-mind_*` tools (49 of them) | yes | Folded into `workflow.run` with a `topology` option |
| Per-CLI-vendor tools (`github_*`, `flow-nexus_*`, etc.) | yes | Belong in plugins, not core |
| `daa_*` (15 tools) | yes | Defer to v2 — DAA is its own concern |
| `neural_train` / `neural_predict` / `neural_patterns` | yes | Folded into `intel.*` |
| `task.*` (create/list/complete) | yes | Tasks live in the host CLI (Claude Code's TaskCreate, etc.) — Ruflo doesn't own them |
| `swarm.init` / `swarm.spawn` / `swarm.status` | yes | Folded into `workflow.run` + `agent.*` |
| `claims.*` (4 tools) | yes | Plugin in v2 if needed |
| `embeddings.*` (4 tools) | yes | Internal to `ruvector-core`, not an MCP surface |
| Per-provider tools (`stripe_*`, `gemini_*` integrations, etc.) | yes | Plugin space |

---

## 2. The 12 agent archetypes + traits

Naming: **archetype**, lowercase, single word. Behavior variants are **traits**, passed as `--trait=<name>` and composable. The current 60+ "agent types" collapse into these 12 + a trait vocabulary.

| # | Archetype | Purpose | Default model tier |
|---|---|---|---|
| 1 | `coder` | Implementation | Tier 2 (Haiku/Codex-mini) or Tier 3 |
| 2 | `reviewer` | Code review, quality, style | Tier 3 |
| 3 | `tester` | Test design + execution | Tier 2 |
| 4 | `researcher` | Investigation, codebase discovery | Tier 2 |
| 5 | `architect` | System / API design | Tier 3 |
| 6 | `planner` | Task decomposition, GOAP | Tier 3 |
| 7 | `security` | Threat modeling, vuln review | Tier 3 |
| 8 | `perf` | Benchmark, profiling, optimization | Tier 2/3 |
| 9 | `devops` | CI/CD, infra, deployment | Tier 2 |
| 10 | `data` | Schemas, migrations, queries | Tier 2 |
| 11 | `docs` | Documentation, API specs | Tier 2 (Haiku) |
| 12 | `coordinator` | Swarm queen / pipeline driver | Tier 3 |

### Trait vocabulary (composable, multi-select)

Traits modify the **prompt + tool allow-list + model preference**. Same archetype, different behavior.

| Trait | Applies to | Effect |
|---|---|---|
| `--trait=tdd` | `coder`, `tester` | London-school mock-first cycle |
| `--trait=backend` | `coder`, `architect` | Server / API focus |
| `--trait=frontend` | `coder`, `reviewer` | UI / a11y / responsive focus |
| `--trait=mobile` | `coder` | iOS / Android / React Native |
| `--trait=ml` | `coder`, `researcher` | Model training / inference |
| `--trait=domain` | `architect` | DDD bounded contexts |
| `--trait=cloud` | `architect`, `devops` | AWS / GCP / k8s |
| `--trait=db` | `data` | Schema + perf + migrations |
| `--trait=audit` | `security`, `reviewer` | Adversarial verification mode |
| `--topology=hierarchical` | `coordinator` | Queen + workers |
| `--topology=mesh` | `coordinator` | Peer network |
| `--topology=adaptive` | `coordinator` | Dynamic switching |

### What's deleted vs the current 60+ types

| Cut | Folds into |
|---|---|
| `system-architect`, `architecture`, `architect`, `repo-architect`, `v3-queen-coordinator`, `sparc:architect`, `sparc-coord/architecture` | `architect` (+ traits) |
| `researcher`, `scout-explorer`, `deep-researcher`, `general-purpose`, `dossier-investigator`, `graph-navigator` | `researcher` |
| `coder`, `backend-dev`, `mobile-dev`, `ml-developer`, `cicd-engineer`, `sparc-coder`, `sparc:code` | `coder` (+ traits) |
| `reviewer`, `code-review-swarm`, `pr-manager`, `analyst`, `code-analyzer` | `reviewer` |
| `tester`, `tdd-london-swarm`, `production-validator`, `test-architect`, `test-long-runner` | `tester` (+ traits) |
| `security-architect`, `security-auditor`, `security-manager`, `sparc:security-review` | `security` |
| `perf-analyzer`, `performance-benchmarker`, `performance-optimizer`, `performance-engineer` | `perf` |
| `hierarchical-coordinator`, `mesh-coordinator`, `adaptive-coordinator`, `collective-intelligence-coordinator`, `swarm-init`, `swarm-coordinator`, `byzantine-coordinator`, `raft-manager`, `gossip-coordinator`, `quorum-manager`, `consensus-coordinator` | `coordinator` (+ topology) |
| `task-orchestrator`, `goal-planner`, `sublinear-goal-planner`, `code-goal-planner`, `migration-planner`, `planner` | `planner` |
| All vendor-specific agents (`github-modes`, `flow-nexus-*`, `ruflo-*`, `understand-anything:*`, `chrome-devtools-*`, `stripe:*`) | plugin space, not core archetypes |

---

## 3. Hooks: the 8 that survive

Current state: 17–27 hooks depending on which doc you trust. Cut to **8**, all emit through `hooks.pre` / `hooks.post` MCP tools with a `kind` discriminator.

| # | Hook | Kind | Fires on |
|---|---|---|---|
| 1 | `pre-task` | task | Before any Claude Code task start |
| 2 | `post-task` | task | After completion (success/fail outcome → SONA) |
| 3 | `pre-edit` | edit | Before file write/edit |
| 4 | `post-edit` | edit | After file write/edit (codemod tier + learning signal) |
| 5 | `pre-command` | command | Before shell exec (risk assessment) |
| 6 | `post-command` | command | After shell exec (outcome capture) |
| 7 | `session-start` | session | Boot — restore session, prime memory |
| 8 | `session-end` | session | Persist `.rvf` snapshot, consolidate |

**Deleted:** `pre-bash` / `post-bash` (aliases), `route-task` (alias), `notify`, `metrics`, `list`, `progress`, `statusline`, `coverage-route`, `coverage-suggest`, `coverage-gaps`, `pretrain`, `build-agents`, `explain`, `transfer`, all `intelligence trajectory-*` sub-hooks (folded into `hooks.post` outcome capture).

**Coverage features** (coverage-route etc.) — defer entirely to a v2 plugin, not a core hook.

---

## 4. Workers: 0 in v1

The current 12-worker in-process daemon model is deleted in v1. Reasons:

- It's the source of the Windows persistence bug (#1766) and the headless race (#2251).
- It blocks every async operation behind a daemon that may or may not be running.
- The work it does (consolidate, optimize, audit, map, deepdive, document, refactor, benchmark, testgaps) is better expressed as `workflow.run` invocations triggered by `hooks.post`.

**v2 path** if needed: a SQLite-backed durable work queue with stateless tick workers. Not built in v1.

---

## 5. Plugin directory: single canonical layout

Discovery order (first match wins per plugin name):

```
1. ./.ruflo/plugins/<name>/         ← project-local
2. ~/.ruflo/plugins/<name>/         ← user-global
3. $RUFLO_HOME/plugins/<name>/      ← env override (CI)
4. <workspace>/crates/ruflo-plugin-host/registry/<name>/ ← built-in
```

Per-plugin file layout (one canonical form, no aliases):

```
<name>/
├── plugin.toml          ← manifest (Rust-native; no plugin.json / .claude-plugin.json)
├── README.md            ← user-facing
├── agents/*.md          ← Claude Code agent definitions (markdown + YAML frontmatter)
├── skills/*/SKILL.md    ← Claude Code skills
├── commands/*.md        ← slash commands
└── hooks/*.toml         ← hook bindings (optional)
```

`plugin.toml` minimal schema:

```toml
[plugin]
name        = "ruflo-graph-intelligence"
version     = "0.2.0"
description = "..."
license     = "MIT"
authors     = ["..."]

[capabilities]
agents   = ["graph-navigator"]
skills   = ["kg-extract", "kg-traverse"]
commands = ["kg"]
hooks    = []

[compat]
ruflo_min  = "4.0.0"
```

**What's deleted:**
- The IPFS-pinned `LIVE_REGISTRY_CID` indirection
- The hardcoded `demoPluginRegistry` fallback
- `/v3/plugins/*` directory entirely (merge survivors into the new canonical location)
- `plugin.json`, `.claude-plugin/plugin.json`, marketplace metadata files outside the plugin dir

IPFS remains as a **release-time CDN** for installable bundles (`ruflo plugin install <name>` fetches a signed tarball) — never as the source of truth.

### Plugins that survive the merge

Of the 34 in `/plugins/ruflo-*` + 17 in `/v3/plugins/*`, keep only those with: tests, an ADR, a commit in the last 90 days, and a clear v1 value. Inventory pass is part of Phase 0.

**Provisional keep list (subject to audit):**
- `ruflo-core` (split — most folds into the binary)
- `ruflo-rag-memory` (becomes `ruflo-plugin-rag`)
- `ruflo-knowledge-graph` (becomes `ruflo-plugin-kg`)
- `ruflo-graph-intelligence`
- `ruflo-browser` (Playwright integration, useful)
- `ruflo-goals` (GOAP planner)
- `ruflo-sparc` (methodology)
- `ruflo-workflows` (GAIA + workflows)
- `ruflo-cost-tracker`
- `ruflo-observability`
- `ruflo-security-audit`

**Provisional drop list:**
- `ruflo-iot-cognitum` (no test coverage)
- `ruflo-market-data` (external feeds not in CI)
- `ruflo-neural-trader` (out of project scope)
- `ruflo-daa` (defer)
- `ruflo-jujutsu` (defer)
- `ruflo-aidefence` (defer — fold into `security` archetype)
- All vendor mirrors of upstream (`@claude-flow/plugin-prime-radiant`, `gastown-bridge`, `agentic-qe`, etc.) — let upstream publish their own plugin
- `/v3/plugins/*` experimental ones: `quantum-optimizer`, `hyperbolic-reasoning`, `cognitive-kernel`, `legal-contracts`, `healthcare-clinical`, `financial-risk` — defer or delete

Net: roughly 11 plugins survive into v1 out of 51 total. Each gets a one-page health check (tests / ADR / last commit / value).

---

## 6. Single CLAUDE.md (autogenerated)

**Source of truth:** `docs/spec/manifest.toml` — the only hand-edited file. Pulls counts from the build (`scripts/inventory.rs` introspects clap + tool registry + hook registry).

**Output:** one `CLAUDE.md` at repo root, target ≤ 8 KB, autogenerated by `cargo run --bin gen-claude-md`. CI fails if the file is edited by hand or exceeds the size cap.

**Allowed sub-CLAUDE.md:** per-crate, ≤ 2 KB, crate-local dev notes only. No project-wide rules duplicated.

**Deleted:**
- The 45 KB root `CLAUDE.md` (current)
- `v3/CLAUDE.md`
- `v3/@claude-flow/cli/CLAUDE.md`
- `CLAUDE.local.md` (move to a user-owned override pattern documented once)
- The 21 KB `AGENTS.md` (folded into the agent archetype docs in `crates/ruflo-host/AGENTS.md` — ≤ 4 KB)

---

## 7. Crate-level scope contract

| Crate | Lines budget | Public API surface |
|---|---|---|
| `ruflo-cli` | ≤ 8 k LOC | clap entry, command dispatch — no business logic |
| `ruflo-mcp` | ≤ 6 k LOC | JSON-RPC stdio loop + the 20 tool handlers |
| `ruflo-host` | ≤ 6 k LOC | `CliHost` trait, Claude + Codex adapters, output normalizer |
| `ruflo-plugin-host` | ≤ 4 k LOC | Discovery, manifest parsing, shell invocation |
| `ruflo-hooks` | ≤ 3 k LOC | The 8 hooks + the SONA wiring |
| `ruflo-session` | ≤ 3 k LOC | `.rvf` write/read, fork, signature verify |

**Total Ruflo-new code budget: ≤ 30 k LOC.** Anything above is a smell that the substrate isn't being used.

**The file-size rule from the current project is enforced this time:** all `.rs` files ≤ 500 lines. CI gate (`cargo fmt` + `clippy` + custom `--max-lines 500` check). No exceptions.

---

## 8. What stays out of v1 entirely

Explicit defer list — none of these are in the v1 scope contract:

- Multi-tenant federation (`ruflo-federation`, WireGuard mesh)
- Marketplace UI / web frontend
- `claims.*` authorization plugin
- `daa_*` distributed-autonomous-agents tools
- Neural training UI / `neural train --epochs N`
- Flash Attention performance claims (until benchmarked in `ruvector-attention`)
- 150x–12,500x HNSW claims (only publish measured numbers from `ruvector-bench`)
- IoT / agentic-robotics / hailo / Pi5 thermal (RuVector keeps these behind features)
- Quantum coherence / consciousness examples (RuVector experimental)
- The 12-worker daemon
- Coverage-aware routing
- The `migrate` v2→v3 command
- All `dual run` / collaboration templates as MCP tools (folded into `workflow.run` with templates as data, not separate tools)
- All `gaia-*` benchmark commands (separate `ruflo-bench` binary in v2)
- Per-vendor plugins (Stripe, Gmail, Drive, etc.) — let vendors ship their own

---

## 9. Open questions to close before Phase 0 sign-off

These block sign-off; assign owners and resolve before any Rust is written.

| # | Question | Owner | Default if unanswered |
|---|---|---|---|
| 1 | Repo name: rUvOS vs ruvflow vs keep "ruflo" | you | **rUvOS** |
| 2 | Does `ruflo-mcp` replace `mcp-brain-server`, or wrap it? | spike | **Replaces, internally `use mcp_brain::*;`** |
| 3 | Final 11-plugin survivor list (Phase 0 audit) | researcher pass | Provisional list in §5 |
| 4 | npm download shim — yes/no | you | **Yes (preserves `npx ruflo`)** |
| 5 | Distribution: install.sh + GitHub releases first; Homebrew/Scoop later? | you | **Yes, ship install.sh + releases at v4.0.0** |
| 6 | Which RuVector `.claude/agents/` survive vs Ruflo's | inventory pass | Ruflo's archetypes (12) win; RuVector's are deleted from `.claude/` (they belong in plugins if at all) |
| 7 | License: RuVector MIT + Ruflo MIT → no conflict; CLA carry-over? | legal | Confirmed compatible, CLA carries over |
| 8 | Versioning: rUvOS v4.0.0 stable from day one (no alpha tags) | you | **v4.0.0-rc.1 → v4.0.0 stable** |

---

## 10. Sign-off

When this ledger is approved, Phase 1 starts. Approval = a tagged commit on the new repo with this file at `docs/spec/scope-ledger-v1.md` and a one-line CHANGELOG entry: *"Phase 0 scope ledger approved — Rust rewrite begins."*

After that point, any addition to the scope requires an ADR. Subtraction requires only a PR.
