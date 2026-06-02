# Phase 0: rUvOS Workspace & Ruflo Scaffolding — Design

> **For agentic workers:** Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task once the implementation plan is written.

**Goal:** Establish a working Cargo workspace that merges curated RuVector crates (as a full copy, not a dependency) with Ruflo's 6 foundational crate stubs, validated by successful compilation and basic module structure.

**Architecture:** Phase 0 is three sequential, validated spikes: (1) audit RuVector's dependency graph to identify the minimal set of crates Ruflo consumes, (2) scaffold the monorepo structure with both substrate and Ruflo layers, (3) validate compilation and CI infrastructure.

**Tech Stack:** Rust 1.70+, Cargo workspace, GitHub Actions CI.

---

## 1. RuVector Dependency Audit Spike

**Objective:** Identify the exact set of RuVector crates to copy into `substrate/`.

**Context:** RuVector has 143 members and 136 crates. Ruflo needs only a curated subset (HNSW, RaBitQ, SONA, .rvf, RuVLLM, Raft, etc.). Copying everything and scoping `default-members` is wasteful and obscures the real dependency boundary.

**Approach:**

1. Clone/access `/mnt/datadisk/repos/rUvnet/RuVector` locally
2. Run `cargo tree --all-features` at the RuVector workspace root to generate the full dependency graph
3. For each capability in the scope ledger (HNSW + ACORN + DiskANN, RaBitQ, SONA, Raft, .rvf, RuVLLM, MCP server, witness chain), trace which crate(s) provide it
4. Build the transitive closure: if HNSW depends on `ruvector-math` and `ruvector-simd`, those must be included too
5. Document in `docs/spec/ruvector-curation.md`: 
   - List of crates to copy (with rationale)
   - Dependency graph excerpt showing why each was chosen
   - Count of total crates (expected: 30-50 of the 136)

**Deliverable:** `docs/spec/ruvector-curation.md` (1-2 KB, structured list + rationale).

**Validation:** Manually verify that the chosen set covers all scope-ledger capabilities.

---

## 2. Monorepo Structure & Scaffolding

**Objective:** Create a working Cargo workspace with Ruflo scaffolding on top of curated RuVector.

### 2.1 Directory Layout

```
ruvos/
├── crates/                          ← Ruflo layer (6 crates, ≤8k LOC each)
│   ├── ruflo-cli/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs               (module stubs)
│   │       ├── main.rs              (entry point, currently empty)
│   │       ├── commands/
│   │       │   ├── mod.rs
│   │       │   ├── init.rs
│   │       │   └── mcp.rs
│   │       └── dispatch/
│   │           └── mod.rs
│   ├── ruflo-mcp/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── server.rs            (JSON-RPC over stdio)
│   │       └── tools/
│   │           ├── mod.rs
│   │           ├── memory.rs
│   │           ├── session.rs
│   │           ├── agent.rs
│   │           ├── hooks.rs
│   │           ├── intel.rs
│   │           ├── plugin.rs
│   │           ├── gov.rs
│   │           └── workflow.rs
│   ├── ruflo-host/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── host.rs              (CliHost trait)
│   │       └── adapters/
│   │           ├── mod.rs
│   │           ├── claude.rs
│   │           └── codex.rs
│   ├── ruflo-plugin-host/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── discovery.rs         (plugin discovery, manifest parsing)
│   │       └── registry/
│   │           └── mod.rs
│   ├── ruflo-hooks/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       └── hooks/
│   │           ├── mod.rs
│   │           ├── pre.rs
│   │           ├── post.rs
│   │           └── route.rs
│   └── ruflo-session/
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── rvf.rs               (.rvf container I/O)
│           ├── fork.rs              (COW fork)
│           └── verify.rs            (signature verification)
│
├── substrate/                       ← RuVector curated crates (copied from RuVector repo)
│   ├── ruvector-core/
│   ├── ruvector-acorn/
│   ├── ruvector-rabitq/
│   ├── sona/
│   ├── rvf/
│   ├── rvf-crypto/
│   ├── ruvllm/
│   ├── ruvector-raft/
│   ├── ruvector-replication/
│   └── ... (others from curation audit)
│
├── Cargo.toml                       ← Workspace root
├── Cargo.lock
├── docs/
│   ├── spec/
│   │   ├── scope-ledger-v1.md       (existing)
│   │   ├── rewrite-summary.md       (existing)
│   │   └── ruvector-curation.md     (Phase 0 audit output)
│   └── superpowers/
│       └── specs/
│           └── 2026-06-02-phase0-ruvos-workspace.md
├── .github/
│   └── workflows/
│       └── ci.yml                   (Phase 0 CI: build + clippy + fmt)
├── .gitignore
├── CLAUDE.md                        (updated with Phase 0 notes)
└── README.md                        (project overview)
```

### 2.2 Root Cargo.toml

```toml
[workspace]
members = ["crates/*", "substrate/*"]
default-members = [
    "crates/ruflo-cli",
    "crates/ruflo-mcp",
    "crates/ruflo-host",
    "crates/ruflo-plugin-host",
    "crates/ruflo-hooks",
    "crates/ruflo-session",
    # RuVector crates Ruflo directly uses (from curation audit)
    "substrate/ruvector-core",
    "substrate/ruvector-acorn",
    "substrate/ruvector-rabitq",
    "substrate/sona",
    "substrate/rvf",
    "substrate/rvf-crypto",
    "substrate/ruvllm",
    # ... others from curation audit
]

[workspace.package]
version = "4.0.0-rc.1"
edition = "2021"
license = "MIT"
authors = ["rUvOS contributors"]
```

### 2.3 Ruflo Crate Scaffolding

Each Ruflo crate gets:
- A `Cargo.toml` with minimal dependencies (only what Phase 0 needs; Phase 1+ will add more)
- A `src/lib.rs` with module declarations matching the scope ledger

**Example: `crates/ruflo-mcp/Cargo.toml`**

```toml
[package]
name = "ruflo-mcp"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

**Example: `crates/ruflo-mcp/src/lib.rs`**

```rust
pub mod server;
pub mod tools;

// Stub for Phase 1 implementation
pub async fn start() {
    println!("MCP server not yet implemented");
}
```

**Example: `crates/ruflo-mcp/src/tools/mod.rs`**

```rust
pub mod memory;
pub mod session;
pub mod agent;
pub mod hooks;
pub mod intel;
pub mod plugin;
pub mod gov;
pub mod workflow;
```

Each tool module (e.g., `tools/memory.rs`) is a stub:

```rust
// Stub: will be implemented in Phase 2+
pub struct MemorySearchTool;
```

---

## 3. Validation & CI Infrastructure

### 3.1 .gitignore

Standard Rust ignores:

```
target/
Cargo.lock
*.rs.bk
.DS_Store
.vscode/
.idea/
```

### 3.2 GitHub Actions CI (`.github/workflows/ci.yml`)

```yaml
name: CI

on:
  push:
    branches: [main, master]
  pull_request:
    branches: [main, master]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo build --workspace
      - run: cargo clippy --all-targets --all-features -- -D warnings
      - run: cargo fmt -- --check
      - run: cargo test --lib

  # Phase 0: just validate compilation
  # Phase 1+: add integration tests, MCP server roundtrip, etc.
```

### 3.3 Validation Checklist

Phase 0 is complete when:

- ✓ `cargo build --workspace` succeeds
- ✓ `cargo clippy` passes with no warnings
- ✓ `cargo fmt --check` passes
- ✓ All 6 Ruflo crates have module structure (not blank files)
- ✓ Curated RuVector crates are copied and included in `default-members`
- ✓ `docs/spec/ruvector-curation.md` documents the audit
- ✓ CI pipeline runs and passes
- ✓ Git history: one clean commit with Phase 0 structure

---

## 4. Success Criteria

**Deliverables:**

1. **Workspace structure** — `ruvos/` with `crates/`, `substrate/`, root `Cargo.toml`
2. **Ruflo scaffolding** — 6 crates with module stubs matching scope ledger
3. **RuVector curated copy** — minimal set of crates copied to `substrate/`
4. **Curation audit** — `docs/spec/ruvector-curation.md` documenting choices
5. **CI pipeline** — GitHub Actions validating build, clippy, fmt, test
6. **Documentation** — CLAUDE.md updated with Phase 0 completion notes

**Validation:**

- Compilation: `cargo build --all-features` clean
- Linting: `cargo clippy` with no warnings
- Formatting: `cargo fmt --check` passes
- Module structure: all 6 Ruflo crates have `src/lib.rs` with module declarations
- Git: clean commit tagged with Phase 0 completion

---

## 5. Handoff to Phase 1

Once Phase 0 is merged:

- **Phase 1 starts:** Merge into RuVector workspace (if not already done), wire up `Cargo.toml` across both repos, get CI fully green
- **Phase 2 starts:** Implement `ruflo mcp serve` with hello-world tool, test end-to-end with Claude Code CLI
- **Future phases:** Fill in tool implementations, agent archetypes, hooks, etc. in tasks that each fit in ≤500 line files

The Phase 0 workspace is the foundation. Phase 1-7 builds the actual functionality on top of it.

---

## 6. Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| RuVector dependency graph is too complex to audit in 2 days | Use `cargo tree --prune` to exclude optional features; focus on scope-ledger capabilities only |
| Copying RuVector introduces circular/conflicting dependencies | Validate with `cargo check` after copy; RuVector is already a working workspace |
| Ruflo crates are too stubby to be useful | Module structure matches scope ledger exactly; Phase 1 starts with concrete implementations |
| CI takes too long with full RuVector | `default-members` scoping ensures CI only builds what Ruflo uses |

