# Ruflo v4 — Rust Rewrite + RuVector Merge: Decision Summary

> Working document capturing the conversation that produced the v4 plan. Carry this into the new project folder as the brief.

---

## 1. Why a rewrite

A deep audit of the current `claude-flow` / `ruflo` repo surfaced structural problems that have been growing for ~6,000 commits:

- **~631 k LOC of TypeScript** across `v3/` (1,435 files). Files like `commands/hooks.ts` (5,331 LOC) and `mcp-tools/hooks-tools.ts` (4,599 LOC) violate the project's own 500-line rule.
- **Three published npm packages** from one repo (`@claude-flow/cli`, `claude-flow`, `ruflo`) with three drifting `overrides` blocks and four bin names. Documented as a known release footgun (#2112).
- **Two parallel plugin directories** (`/plugins/ruflo-*` with 34 entries, `/v3/plugins/*` with 17). Marketplace registry on IPFS, hardcoded fallback in source — four sources of truth.
- **323 MCP tools** and **60+ agent types** with obvious aliases (`system-architect` / `architect` / `architecture`; `researcher` / `scout-explorer` / `general-purpose`). Discoverability is dead.
- **Performance claims marked unverified in the same docs that publish them** — HNSW "150x–12,500x NOT reproduced," Flash Attention "no benchmark exists." Trust risk.
- **Daemon** has a recent fix for a headless race (#2251) and an open Windows persistence bug (#1766) — the in-process worker model is fragile.
- **Skipped integration tests** (#1872) cover real production bugs that ship silently each release.
- **Only one Rust crate exists** (`ruflo-federation-peer`), yet the whole "performance" pitch rides on third-party NAPI bridges (`ruvector@0.2.27`, `@ruvector/*`, `agentdb`).

User constraint: **no v2 legacy support needed** — free to break anything.

---

## 2. External validation (Prism MCP consensus)

Asked Gemini 3 Pro and GPT‑5.5 the same question independently. Both scored **8/10 confidence** with strongly overlapping verdicts:

- **Do a Rust-native rewrite of CLI + MCP server + state store + plugin host.** Treat TS as a temporary distribution wrapper only.
- **Don't port — cut.** A 1:1 port of 323 tools and 60+ agents is the failure mode. The value is in the scope cut, not the language change.
- **Single static binary** for distribution. No Node runtime required for any Claude Code surface.
- **Keep Claude Code plugin/skill format intact** — markdown + YAML frontmatter parsed natively in Rust; shell commands invoked via `tokio::process::Command`. No embedded JS runtime, no WASM-for-plugins (until later).
- **Crate / dep choices both models converged on:** tokio, clap, serde + schemars, tracing, rusqlite, hnsw_rs, gray_matter / serde_yaml for frontmatter.
- **MCP SDK:** roll a thin JSON-RPC over `tokio::io::stdin/stdout` rather than wait on an unstable third-party crate. MCP is small enough to own.
- **Embeddings:** don't ship local inference in v1. Call provider APIs / external commands. Revisit `fastembed-rs` / `ort` / `candle` later. (GPT-5.5's call — adopted.)

**Failure modes both flagged:**
1. **Scope preservation** — porting everything kills the rewrite at 80 %.
2. **Claude Code integration drift** — must test stdio MCP + skill format end-to-end *from Claude Code* on day 1, not at the end.
3. **In-memory daemon graph state** (Gemini) — push state to SQLite (or RVF), never hold a mutable graph across workers.

---

## 3. The pivotal discovery: RuVector

`/mnt/datadisk/repos/rUvnet/RuVector` is **already a pure-Rust workspace with 143 members, 136 crates, ~4 k .rs files**, containing the exact substrate Ruflo today reaches through NAPI:

| Ruflo needs | RuVector already ships |
|---|---|
| HNSW + ACORN + DiskANN | `ruvector-core`, `ruvector-acorn`, `ruvector-diskann` |
| RaBitQ / Int8 quantization | `ruvector-rabitq` |
| SONA self-learning | `crates/sona` |
| Attention (Flash + 50 mechanisms) | `ruvector-attention`, `ruvector-attn-mincut` |
| GNN / graph RAG / Cypher | `ruvector-gnn`, `ruvector-graph` |
| Model router | `ruvector-router-core` |
| Raft / replication / cluster | `ruvector-raft`, `ruvector-replication`, `ruvector-cluster` |
| Local LLM | `ruvllm` |
| Cryptographic witness chain | `rvf-crypto` (ML-DSA-65 + Ed25519) |
| Session container | `rvf` (`.rvf` files — Git-like COW, signed segments) |
| MCP server (Rust) | `mcp-brain-server`, `mcp-gate` |
| `.claude/agents/` definitions | already present |

This is **what Ruflo today calls through NAPI**. Merging it native turns the Rust rewrite from "build it" into "wire it up."

---

## 4. Decision: merge Ruflo into the RuVector workspace

| Option | Verdict |
|---|---|
| **Merge Ruflo into RuVector's Cargo workspace** | **Chosen.** One repo, one CI, one supply chain, atomic refactors. |
| Path dep across sibling repos | Worst of both worlds long-term. |
| Pin `ruvector-core` from crates.io | Reintroduces the dependency-drift the rewrite is escaping. |

**Repo:** stays at `rUvnet/RuVector` (or renamed to `ruvnet`); npm package names (`ruvector`, `ruflo`) unchanged.

**Positioning:** *RuVector is the self-learning vector + graph + local-AI substrate. Ruflo is the agent orchestration layer that runs on top of it.*

**Legacy TS code:** moved to `legacy/ts-claude-flow/`, frozen, deleted in v2 of the rewrite. Used only as a reference for what individual tools did.

---

## 5. Target architecture

### 5.1 New crates Ruflo adds to the RuVector workspace

```
crates/
  ruflo-cli/            ← clap-based CLI shell (ruflo init/mcp/agent/...)
  ruflo-mcp/            ← MCP server: JSON-RPC over stdio + tool registry
  ruflo-host/           ← CliHost trait + Claude/Codex/Gemini adapters
  ruflo-plugin-host/    ← markdown/YAML skill discovery + shell exec
  ruflo-hooks/          ← Ruflo's hooks compiled into the binary
  ruflo-session/        ← session state persisted as .rvf containers
```

Everything else is `use ruvector_*;` or `use sona::*;` or `use rvf::*;`.

### 5.2 Workspace policy

- `default-members` scoped so Ruflo's CI only builds what it consumes from RuVector. Experimental crates (consciousness examples, quantum coherence, etc.) stay behind features.
- Atomic refactors across both projects land in one PR.
- One CI, one release cadence, one binary.
- The live contract manifest is generated from the Rust workspace and stored at `docs/contracts/contract-manifest.json`; docs and CI should verify against it instead of hand-maintaining tool lists.

### 5.3 Distribution

Single static binary `ruflo`. Distribution paths:

| Path | Use |
|---|---|
| GitHub releases + `curl …/install.sh \| sh` | Primary. Matches uv / bun / ripgrep. |
| Homebrew / Scoop / winget | Add post-launch. |
| Optional ~30-line npm postinstall shim | Keeps `npx ruflo` working. Same pattern as esbuild / swc / biome / turbo. **Not** TypeScript code. |
| `cargo install ruflo` | Skip — bad UX for non-Rust users. |

---

## 6. Multi-CLI support (Claude Code / Codex / Gemini)

Two compositional directions, both delivered:

### 6.1 Direction A — Ruflo as MCP server consumed by any CLI

MCP is a standard. The same binary registers into all three:

| CLI | Registration |
|---|---|
| Claude Code | `claude mcp add ruflo -- ruflo mcp serve` |
| Codex CLI | `~/.codex/config.toml` → `[mcp_servers.ruflo]` |
| Gemini CLI | `~/.gemini/settings.json` → `mcpServers.ruflo` |

Zero code branching. This is essentially free.

### 6.2 Direction B — Ruflo as orchestrator that drives any CLI

The `CliHost` trait in `ruflo-host`:

```rust
#[async_trait]
pub trait CliHost: Send + Sync {
    fn name(&self) -> &'static str;          // "claude" | "codex" | "gemini"
    fn available_models(&self) -> &[ModelSpec];
    async fn run(&self, req: AgentRequest) -> Result<AgentOutput>;
    fn stream(&self, req: AgentRequest)
        -> Pin<Box<dyn Stream<Item = AgentEvent> + Send>>;
}

pub struct AgentRequest {
    pub prompt: String,
    pub model: Option<String>,
    pub session_id: Option<Uuid>,       // Ruflo owns sessions, not the CLI
    pub allowed_tools: Vec<String>,
    pub budget_usd: Option<f64>,
    pub working_dir: PathBuf,
}
```

| Capability | Claude | Codex | Gemini |
|---|---|---|---|
| Headless | `claude -p` | `codex exec` | `gemini -p` |
| Model | `--model haiku/sonnet/opus` | `--model gpt-5.4-mini/...` | `--model gemini-3-pro-preview` |
| Output | `--output-format stream-json` | own format | text (needs normalizer) |
| Session | `--session-id` / `--resume` / `--fork-session` | own | none — Ruflo simulates |
| Budget | `--max-budget-usd` | n/a | n/a |
| Allowed tools | `--allowedTools` | restricted differently | n/a |
| Auth | `claude login` | `OPENAI_API_KEY` | `GEMINI_API_KEY` |

Three consequences baked into the design:

1. **Ruflo owns sessions** in `.rvf` / SQLite, not the CLI. Dodges Gemini's lack of session support; enables fork/resume across hosts.
2. **One output normalizer per host** → unified `AgentEvent` stream.
3. **Auth stays with each CLI** — Ruflo never holds keys. Big security-surface reduction vs. current TS code.

### 6.3 How A and B compose

A Claude agent spawned via Direction B calls Ruflo's MCP tools (Direction A), one of which spawns a Codex agent. Recursion bottoms out on the same `CliHost` trait and the same `.rvf` session store. This is what makes multi-CLI swarms structural, not bolted-on.

### 6.4 Tri-mode collaboration templates

The current `🔵 Claude + 🟢 Codex` dual-mode templates extend to `<host>:<role>:<task>`:

```
feature: claude:architect → codex:coder → gemini:reviewer → claude:tester
```

The 3-tier router (codemod / Haiku / Opus) generalizes to (codemod / fast-model-on-any-host / frontier-on-any-host). SONA's learning loop already shaped for this — the data shape is identical.

---

## 7. Aggressive scope cuts (do these before any Rust is written)

Both consensus models flagged this as the difference between success and a stalled 80 % rewrite.

| Today | Target for v1 |
|---|---|
| 323 MCP tools | ~20 in v1, ~80 ceiling. Domains: `memory.*`, `swarm.*`, `hooks.*`, `agent.*`, `plugin.*`, `intel.*`, `gov.*` |
| 60+ agent types | ~12 archetypes + trait modifiers (`coder --trait=tdd`, `coder --trait=mobile`) |
| 17–27 hooks (docs disagree) | Single registry, single count, autogenerated from source |
| 12 background workers | Daemon optional. Workers consume from SQLite-backed durable queue. |
| 4 sources of plugin truth | One: `crates/ruflo-plugin-host/registry/` → pushed to IPFS at release time as CDN |
| 3 published npm packages | One published artifact: `ruflo` binary + optional thin npm shim |
| Multiple CLAUDE.md files (~25 k tokens) | One canonical CLAUDE.md ≤ 8 KB, autogenerated from `docs/spec/` |

**Deferred out of v1 entirely:** marketplace, SONA marketing claims, Flash Attention claims, duplicated plugin registries, `v3:migrate` command, all v2 compat hooks (`pre-bash`, `post-bash`, `route-task`).

---

## 8. Phase plan

| Phase | Scope | Duration |
|---|---|---|
| **0. Scope ledger** | Pick the ~20 MCP tools, ~12 agent archetypes, single plugin dir, single CLAUDE.md. Done in current TS repo or in a doc. **No Rust yet.** | 3–5 days |
| **1. Substrate handshake** | Merge Ruflo into RuVector workspace. Create six new crates as skeletons. CI green on empty Rust. | 1 week |
| **2. MCP server day-1** | `ruflo mcp serve` ships a single hello-world tool registered into Claude Code, Codex CLI, and Gemini CLI. The integration-drift tripwire. | 1 week |
| **3. Plugin host** | Markdown + YAML frontmatter discovery; shell command execution via tokio. Ship Claude Code skills/agents/commands compatibility. | 1 week |
| **4. Hooks + daemon** | Port the ~20 chosen hooks to Rust. Replace in-process daemon with SQLite-backed queue + worker pool. Windows daemon bug fixed by design. | 2 weeks |
| **5. Memory + session** | `ruflo-session` writing `.rvf` containers; HNSW + RaBitQ via `ruvector-core`. Witness chain via `rvf-crypto` — kills the manifest-drift bug. | 2 weeks |
| **6. CliHost adapters** | Claude + Codex adapters with normalized event streams. Gemini deferred to 6.1 once Claude/Codex are solid. | 2 weeks |
| **7. Cutover + deprecation** | npm download shim ships. TS code moves to `legacy/`. v4 tag. | 1 week |

**Rough total:** ~8–12 weeks for a focused team.

The reason the timeline is short: 80 % of the Rust (vector search, learning, attention, witness chain, local LLM, .rvf format) is already shipped in RuVector. Ruflo's rewrite is six small crates of orchestration glue, not a from-scratch system.

---

## 9. Specific risks to manage

| Risk | Mitigation |
|---|---|
| Scope creep — porting old tools "just in case" | Phase 0 scope ledger is the contract; everything not on it is deleted, not deferred |
| Claude Code integration drift | Day-1 tripwire in Phase 2 — round-trip stdio MCP from real `claude` CLI |
| RuVector workspace build slow | `default-members` scoped to crates Ruflo consumes; experimental crates behind features |
| `mcp-brain-server` vs `ruflo-mcp` overlap | Decision spike: `ruflo-mcp` *uses* brain/gate crates internally; one MCP server registered, not two |
| Gemini CLI output unstructured / no sessions | Gemini deferred to phase 6.1; Claude + Codex first; Ruflo simulates Gemini sessions in `.rvf` |
| In-memory daemon graph state (Gemini's flag) | All state in `.rvf` containers + SQLite. Workers are stateless ticks. |
| Brand confusion (RuVector vs Ruflo) | One-line positioning above; documented in both repos' README |
| Coupling once monorepo'd | Treated as a benefit — atomic landing; if a regression appears in RuVector core, Ruflo's tests catch it before publish |

---

## 10. Open decisions to make before Phase 0 ends

1. **Repo name** — keep `rUvnet/RuVector`, or rename to `rUvnet/ruvnet`?
2. **MCP server merge** — does `ruflo-mcp` replace `mcp-brain-server`, or compose with it?
3. **npm download shim** — ship one to preserve `npx ruflo`, or hard-cutover to `curl … | sh`?
4. **Which existing RuVector `.claude/agents/` survive** — that directory and the current Ruflo agent set overlap heavily.
5. **License posture** — RuVector is MIT, current Ruflo is MIT; no conflict, but confirm contributor agreements carry over on merge.
6. **The 20 v1 MCP tools** — produce the actual list as Phase 0's main artifact.
7. **The 12 agent archetypes** — same, as Phase 0's second artifact.

---

## 11. One-line summary

**Ruflo v4 = a small Rust orchestration layer (six crates) merged into the RuVector workspace, exposing one MCP server to Claude / Codex / Gemini, with markdown skill compatibility, `.rvf` cognitive containers for state, and a ruthless scope cut from 323 tools / 60+ agents / 631 k LOC to roughly 20 tools / 12 archetypes / under 50 k LOC of new Rust on top of the substrate that already exists.**
