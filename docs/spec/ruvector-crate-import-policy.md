# rUvOS — RuVector Crate Import Policy

> Which RuVector crates rUvOS pulls into its default workspace, which it defers behind feature flags, and which it never touches. This is the import contract for the Phase 1 merge.
>
> Companion to `scope-ledger-v1.md`. If a RuVector capability isn't justified by an entry in the v1 scope ledger, it doesn't get imported.

---

## 0. Principles applied

1. **Every imported crate must trace to a specific v1 ledger entry.** No "looks useful." If it can't be tied to a tool, archetype, hook, or scope-ledger §7 capability, it stays in RuVector.
2. **Workspace default-members stays small.** Imported crates are *available*; only the few that build into the `ruflo` binary by default count toward CI cost. The rest live behind cargo features so RuVector keeps them buildable without burdening rUvOS CI.
3. **No NAPI, no WASM, no Node bridges.** rUvOS is a native binary. Every `*-wasm` and `*-node` crate is excluded by definition — that's RuVector's distribution problem, not ours.
4. **One indexer, one learner, one container.** Where RuVector ships multiple implementations of the same primitive (HNSW vs DiskANN vs ACORN; SONA vs GNN), v1 picks one and defers the others until a benchmark proves a workload-specific reason to add a second.
5. **Library, not server.** Where RuVector ships both a library crate and a binary server crate (`mcp-brain` + `mcp-brain-server`), import the library and re-expose its capability through the single `ruflo-mcp` server. Never run two MCP servers.

---

## 1. Tier 1 — Direct dependencies of `ruflo-*` crates (must import)

These are hard requirements for the 20 MCP tools / 8 hooks. Listed with the exact ledger entry that forces them in.

| RuVector crate | Used by | Justification (ledger ref) |
|---|---|---|
| **`ruvector-core`** | `ruflo-mcp`, `ruflo-session` | HNSW index + persistence. Powers `memory.search/store/retrieve/list` (tools 1–4). Substrate-defining. |
| **`ruvector-rabitq`** | `ruflo-session` | 32× compression at 0.60ms/query (measured). Without it, `.rvf` session memory grows linearly and the only honest substrate perf claim disappears. |
| **`sona`** | `ruflo-hooks`, `ruflo-mcp` | The learning loop. Powers `intel.pattern_search/store` (tools 14–15) and the `hooks.post` outcome signal (hooks 2/4/6/8). Substrate-defining. |
| **`ruvector-router-core`** | `ruflo-hooks` | Powers `hooks.route` (tool 13). The 3-tier routing the ledger preserves runs here. |
| **`rvf`** | `ruflo-session`, `ruflo-mcp` | Cognitive container format. Powers `session.create/resume/fork` (tools 5–7) **and** fixes the witness manifest drift bug (#2047) via `rvf-crypto`'s signed segment chain — the basis for `gov.witness_verify` (tool 18). |
| **`ruvector-metrics`** | all `ruflo-*` | Shared observability primitives. Required so every crate doesn't roll its own tracing schema. |
| **`mcp-brain`** *(library only, not the server binary)* | `ruflo-mcp` | Reference implementation of memory-tool semantics over `ruvector-core`. Import as a library so `ruflo-mcp` doesn't reinvent the same handler shapes. **`mcp-brain-server` and `mcp-gate` are NOT imported** — rUvOS ships one MCP server. |

**Count: 7 crates** that build into `ruflo` by default.

---

## 2. Tier 2 — Likely transitive (audit, then accept)

These are almost certainly pulled in transitively by Tier 1; list them so the dependency graph is explicit and reviewed, not stumbled into.

| Crate | Why it's likely transitive | If not transitive |
|---|---|---|
| `ruvector-collections` | Shared data structures across `core`/`router`/`sona` | Import directly; tiny |
| `ruvector-filter` | Predicate engine for `memory.list` and filtered `memory.search` | Import directly |
| `ruvector-math` | Math primitives for indexing/quantization | Import directly; should be tiny |
| `rvf-crypto` (rvf subcrate) | ML-DSA-65 + Ed25519 segment signatures — required for `gov.witness_verify` | Import as a feature of `rvf` |
| `rvf-cow` (rvf subcrate) | COW branching for `session.fork` (tool 7) | Import as a feature of `rvf` |

**Action:** during Phase 1 merge, run `cargo tree -p ruflo-mcp -p ruflo-session -p ruflo-hooks` and confirm the actual transitive set. Anything in this table that doesn't show up gets a direct dependency line.

---

## 3. Tier 3 — Useful, but deferred behind a cargo feature

These crates stay in the RuVector source tree (which lives inside the rUvOS workspace post-merge) and remain buildable individually, but are **not** in `ruflo`'s default-features. A plugin or a future scope-expanded v1.x can opt them in.

| Crate | Capability | Why deferred | What triggers including it |
|---|---|---|---|
| `ruvector-graph` | Cypher + hyperedges + community detection | v1 ledger has no graph-RAG tool. `intel.*` is pattern-based, not graph-traversal. | `ruflo-plugin-kg` plugin enables `feature = "graph"` |
| `ruvector-gnn` | Graph neural network for learning | SONA covers the v1 learning loop; GNN is a different mechanism not on the ledger | Benchmark showing GNN beats SONA on Ruflo's trajectory data |
| `ruvector-attention` + `ruvector-attn-mincut` | 50+ attention mechanisms incl. Flash Attention | The ledger explicitly defers Flash Attention claims until benchmarked in `ruvector-bench`. Importing without a benchmark would reintroduce the unverified-claim problem the rewrite is fixing. | A green run in `ruvector-bench` with measured numbers we'll quote |
| `ruvector-diskann` | Billion-scale SSD-backed ANN | rUvOS v1 has no billion-scale use case. Adding it is added build/test surface for no v1 win. | A user reports a corpus where HNSW exceeds RAM |
| `ruvector-acorn` | Alternative ANN index | One indexer is enough for v1. | A benchmark in `ruvector-bench` showing it beats HNSW on Ruflo's workload |
| `ruvector-snapshot` | Snapshot/restore primitives | `rvf-cow` likely covers `session.fork` already; double-check before pulling | If `rvf-cow` doesn't give end-to-end snapshot semantics |
| `ruvector-sparsifier` | Graph sparsifier | Defers with `ruvector-graph` | Enabled with graph feature |
| `ruvector-solver` | O(log n) PageRank / sublinear solvers | No v1 tool exercises this | Enabled with graph feature |
| `ruvector-bench` | Benchmark harness | Required for *publishing* numbers but not at runtime. Built in CI, not into the `ruflo` binary. | CI step; ledger §8 explicitly forbids unverified claims |
| `ruvector-profiler` | Profiling | `ruvector-metrics` covers v1 observability | Performance regression triage |

**Count: 10 crates** kept building, opted in only when needed.

---

## 4. Tier 4 — Excluded outright (do not import; do not block builds on)

These are real RuVector products / experiments / hardware integrations / WASM bridges that have no place in rUvOS v1. RuVector keeps owning them; rUvOS's workspace `exclude` list bars them from default builds.

### 4a. All NAPI / WASM bridges — excluded by principle #3
```
ruvector-core-wasm           ruvector-core-node           ruvector-gnn-node
ruvector-acorn-wasm          ruvector-graph-node          ruvector-mincut-node
ruvector-attention-wasm      ruvector-graph-wasm          ruvector-mincut-wasm
ruvector-attention-cli       ruvector-graph-transformer-* ruvector-mincut-brain-node
ruvector-attention-node      ruvector-gnn-wasm            ruvector-diskann-node
ruvector-attention-unified-wasm                           ruvector-cnn-wasm
ruvector-rabitq-wasm         ruvector-router-cli          ruvector-router-ffi
ruvector-router-wasm         ruvector-dag-wasm            ruvector-delta-wasm
ruvector-domain-expansion-wasm                            ruvector-economy-wasm
ruvector-exotic-wasm         ruvector-hyperbolic-hnsw-wasm
ruvector-learning-wasm       ruvector-math-wasm           ruvector-mincut-gated-transformer-wasm
ruvector-mincut-gated-transformer                         ruvector-nervous-system-wasm
ruvector-sparse-inference-wasm                            ruvector-sparsifier-wasm
ruvector-solver-node         ruvector-solver-wasm         ruvector-temporal-tensor-wasm
ruvector-tiny-dancer-wasm    ruvector-tiny-dancer-node    ruvector-verified-wasm
ruvector-cnn-wasm            ruvector-consciousness-wasm  ruvector-wasm
ruvector-node                micro-hnsw-wasm              ruvllm-wasm
```
All NAPI/WASM. rUvOS is native. **Excluded.**

### 4b. Domain-specific products — separate concerns
| Crate(s) | What it is | Why excluded |
|---|---|---|
| `agentic-robotics-*` (6 crates) | Robotics runtime | Not on rUvOS roadmap |
| `hailort-sys`, `ruvector-hailo`, `ruvector-hailo-cluster` | Hailo NPU on Pi 5 | Hardware-specific |
| `ruos-thermal`, `thermorust` | Pi 5 thermal supervisor | Hardware-specific |
| `ruvector-mmwave` | mmWave radar | Hardware-specific |
| `cognitum-gate-kernel`, `cognitum-gate-tilezero` | Cognitum chip product | Separate product |
| `neural-trader-*` (5 crates) | Financial trading | Separate product |
| `prime-radiant` | Separate product | Out of scope |
| `ruQu`, `ruqu-*` (5 crates) | Quantum computing primitives | Research |
| `ruvector-consciousness`, `ruvector-coherence` | Experimental research | Research |
| `ruvector-decompiler` | Binary decompiler | Out of scope |
| `ruvector-economy-*`, `ruvector-kalshi` | Domain-specific | Out of scope |
| `ruvector-fpga-transformer*` | FPGA hardware | Hardware-specific |
| `ruvector-nervous-system*` | Biological metaphor research | Research |
| `ruvector-dither`, `ruvector-robotics`, `ruvector-rairs` | Niche | Out of scope (rairs: investigate but skip) |
| `ruvector-graph-transformer*` | Research crate | Research |
| `ruvector-temporal-tensor` | Temporal tensors | Niche |
| `ruvector-domain-expansion` | Transfer learning across domains | Not on ledger; revisit in v2 |
| `ruvector-sparse-inference` | Sparse model inference | Tied to local LLM (deferred) |
| `ruvector-verified` | Formally verified primitives | Strategic, not v1 critical |
| `ruvector-tiny-dancer-core` | Tiny model | Tied to local LLM (deferred) |

### 4c. Components rUvOS replaces with its own
| Crate | What it is | rUvOS replacement |
|---|---|---|
| `ruvector-cli` | RuVector's own CLI binary | `ruflo-cli` (would collide on binary name) |
| `ruvector-server` | HTTP server | rUvOS is stdio MCP, not HTTP |
| `mcp-brain-server` | MCP server binary using brain library | `ruflo-mcp` (one MCP server, not two) |
| `mcp-gate` | MCP gateway | Same — one server, not two |
| `rvAgent` | Alternative agent runtime | rUvOS's `CliHost` trait is canonical |

### 4d. Local LLM stack — deferred per consensus guidance
| Crate | Why excluded in v1 |
|---|---|
| `ruvllm`, `ruvllm-cli`, `ruvllm_retrieval_diffusion`, `ruvllm_sparse_attention` | GPT‑5.5's explicit consensus guidance: don't own model inference in v1; call provider APIs / external CLIs. The multi-CLI design (Claude/Codex/Gemini) means inference happens in the host CLIs, not in rUvOS. v2 candidate for "local-fallback when no CLI host is configured." |

### 4e. PostgreSQL extension — build complexity
| Crate | Why excluded |
|---|---|
| `ruvector-postgres` | pgrx-based; already excluded from RuVector's default workspace for the same reason. rUvOS v1 stores state in `.rvf` + SQLite, not Postgres. |

### 4f. Investigation-required, defaulting to excluded
These don't have obvious purpose from the name. Default is **exclude** until someone reads the crate and demotes them up a tier.

| Crate | Action |
|---|---|
| `ruvix` | Investigate purpose in Phase 1 day 1; default exclude |
| `ruvector-crv` | Investigate; default exclude |
| `rvm` | Investigate (RVF VM?); default exclude |
| `rvlite` | Investigate (lite RVF?); default exclude |
| `ruvector-rulake` | Investigate (data lake?); default exclude |
| `ruvector-cognitive-container` | Possible overlap with `rvf`; investigate. If it duplicates `rvf`, exclude. |
| `ruvector-delta-consensus / -core / -graph / -index` | Distributed consensus primitives — defer with cluster/raft/replication |
| `ruvector-cluster`, `ruvector-raft`, `ruvector-replication` | Distributed state. v1 is single-machine (the deleted hive-mind tools partly motivated this). Defer to v2 federation. |
| `ruvector-dag` | DAG primitives. Not on ledger. Defer. |
| `ruvector-cnn` | CNN. Not relevant to text-agent v1. Defer. |
| `ruvector-mincut` | Graph mincut. Defers with graph. |
| `profiling` | Generic profiling. Pull via transitive only. |

---

## 5. Summary numbers

| Tier | Crates | What they cost |
|---|---|---|
| **Tier 1 — direct, default-on** | 7 | Build into `ruflo`, every CI run, every release |
| **Tier 2 — transitive (audit, accept)** | ~5 | Pulled in via Tier 1; tiny |
| **Tier 3 — feature-gated, opt-in** | ~10 | Buildable, not built by default — zero ongoing cost unless enabled |
| **Tier 4 — excluded** | ~125 | Not in rUvOS workspace; RuVector still owns them |

**Net:** rUvOS depends on **~12 RuVector crates** in the default build, out of ~143. The other ~130 stay alive in RuVector and ship through their own crates.io / npm channels independently.

---

## 6. Workspace mechanics for the merge

Concrete `Cargo.toml` layout for the merged repo:

```toml
# /Cargo.toml  (rUvOS root)
[workspace]
resolver = "2"

members = [
    # --- ruflo orchestration layer (new) ---
    "crates/ruflo-cli",
    "crates/ruflo-mcp",
    "crates/ruflo-host",
    "crates/ruflo-plugin-host",
    "crates/ruflo-hooks",
    "crates/ruflo-session",

    # --- RuVector substrate (Tier 1) ---
    "crates/ruvector-core",
    "crates/ruvector-rabitq",
    "crates/sona",
    "crates/ruvector-router-core",
    "crates/rvf",                # plus rvf-crypto, rvf-cow as sub-features
    "crates/ruvector-metrics",
    "crates/mcp-brain",          # library only

    # --- Likely transitive (Tier 2) ---
    "crates/ruvector-collections",
    "crates/ruvector-filter",
    "crates/ruvector-math",

    # --- Tier 3 (kept in workspace, opted in via features) ---
    "crates/ruvector-graph",
    "crates/ruvector-gnn",
    "crates/ruvector-attention",
    "crates/ruvector-attn-mincut",
    "crates/ruvector-diskann",
    "crates/ruvector-acorn",
    "crates/ruvector-snapshot",
    "crates/ruvector-sparsifier",
    "crates/ruvector-solver",
    "crates/ruvector-bench",     # CI-only consumer
]

default-members = [
    # Only these build on `cargo build`:
    "crates/ruflo-cli",
    "crates/ruflo-mcp",
    "crates/ruflo-host",
    "crates/ruflo-plugin-host",
    "crates/ruflo-hooks",
    "crates/ruflo-session",
]

exclude = [
    # Every Tier 4 crate listed explicitly here.
    # RuVector continues to maintain them; rUvOS workspace doesn't touch them.
    "vendor/ruvector-source/crates/agentic-robotics-*",
    "vendor/ruvector-source/crates/cognitum-gate-*",
    "vendor/ruvector-source/crates/neural-trader-*",
    "vendor/ruvector-source/crates/ruqu*",
    "vendor/ruvector-source/crates/ruvector-postgres",
    "vendor/ruvector-source/crates/ruvllm*",
    "vendor/ruvector-source/crates/*-wasm",
    "vendor/ruvector-source/crates/*-node",
    # ... (see §4 for the full list)
]
```

`ruflo`'s features:

```toml
# /crates/ruflo-cli/Cargo.toml
[features]
default = []

# Optional substrate capabilities — off in v1, available to plugins/users
graph     = ["dep:ruvector-graph", "dep:ruvector-sparsifier", "dep:ruvector-solver"]
gnn       = ["dep:ruvector-gnn", "graph"]
attention = ["dep:ruvector-attention", "dep:ruvector-attn-mincut"]
diskann   = ["dep:ruvector-diskann"]
acorn     = ["dep:ruvector-acorn"]
```

---

## 7. Open investigations (Phase 1 day-1 spikes)

These can't be settled from the crate list alone. Each is a < 2-hour spike that resolves a Tier 3 / Tier 4 boundary call.

| Spike | Resolves |
|---|---|
| Read `rvf/Cargo.toml` and `rvf-cow/lib.rs` — does `rvf-cow` provide end-to-end snapshot semantics, or do we also need `ruvector-snapshot`? | Tier 2 vs Tier 3 placement of `ruvector-snapshot` |
| Read `mcp-brain/src/lib.rs` and `mcp-brain-server/src/main.rs` — confirm `mcp-brain` is genuinely separable as a library | Validates the "library-only import" decision (§1) |
| Why is `rvf` in RuVector's workspace `exclude` block today? Build complexity? Platform deps? Pre-1.0 churn? | Whether rUvOS's workspace can include it as Tier 1 or must pull it as a path-dep |
| `cargo tree -p ruflo-mcp` after stub crates exist — confirm Tier 2 transitive list | Cuts unnecessary direct deps |
| `rvAgent` — what does it actually do? Trajectory recording? Tool execution? | Possible Tier 3 promotion (e.g., for the SONA trajectory loop) |
| `ruvector-cognitive-container` vs `rvf` — overlap? | If overlap, exclude; if complement, possibly Tier 2 |
| `ruvix`, `ruvector-crv`, `rvm`, `rvlite`, `ruvector-rulake` — what are they? | All currently default-excluded; investigation may promote one |

---

## 8. The critical-thinking summary

The list reduces from ~143 RuVector crates to **7 direct dependencies + ~5 transitive + ~10 feature-gated**. The reasoning that drove the cuts:

1. **Pick one of each.** RuVector has multiple ANN indexes (HNSW, ACORN, DiskANN), multiple attention families, multiple inference paths. v1 picks **HNSW** for ANN and **SONA** for learning. The others stay alive in RuVector and join rUvOS when a measured benchmark justifies them.

2. **The biggest temptation is the local LLM stack** (`ruvllm` and friends). It looks like a moat — "we own inference." But the multi-CLI design *delegates* inference to Claude/Codex/Gemini by construction. Bringing `ruvllm` into v1 reintroduces the exact stack-too-tall problem the rewrite is meant to fix. **Hold the line: defer to v2.**

3. **The second biggest temptation is graph-RAG** (`ruvector-graph`, `-gnn`, `-sparsifier`, `-solver`, `-mincut`). It's RuVector's flagship capability and was the headline of recent BEIR commits in the old repo. The v1 ledger deliberately doesn't have a graph-traversal MCP tool — `intel.*` is pattern-based. Including the graph stack in default-members would force every Ruflo CI run to compile ~10 crates that no v1 tool calls. **Feature-gate it; the `ruflo-plugin-kg` plugin opts in.**

4. **All `-wasm` and `-node` crates are categorically out.** Together they're roughly half the workspace. Every one of them exists to bridge into a JS/Node/browser runtime that rUvOS doesn't have. Excluding them is the single biggest reason the rUvOS workspace is small.

5. **The MCP-server question is real.** RuVector already ships `mcp-brain-server` and `mcp-gate`. The decision in the scope ledger (open question #2) was "replace, internally `use mcp_brain::*;`." This policy enforces that: `mcp-brain` is imported as a *library*, the `*-server` binaries are excluded. One MCP server, one tool registry, one schema source.

6. **`rvf` is in RuVector's workspace `exclude` today** — that's a deliberate signal from the RuVector authors that needs investigating before we put it in Tier 1. If it's excluded because of platform deps or build complexity, we own the same complexity. If it's excluded because it's pre-1.0 churn, that's tolerable (rUvOS v4.0.0 is the moment to stabilize it). **Day-1 spike in Phase 1.**

7. **The default-members list is the budget enforcement.** Listing a crate as a workspace member but keeping it out of `default-members` means it stays buildable on demand but never blocks the rUvOS release. This is how RuVector's experimental sprawl coexists with rUvOS's tight scope.
