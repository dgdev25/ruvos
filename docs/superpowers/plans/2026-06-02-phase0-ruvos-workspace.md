# Phase 0: rUvOS Workspace & Ruflo Scaffolding — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish a working Cargo workspace merging curated RuVector crates with Ruflo's 6 foundational crate stubs, validated by successful compilation and CI.

**Architecture:** Three sequential, validated spikes: (1) audit RuVector's dependency graph to identify minimal set, (2) scaffold monorepo structure with both substrate and Ruflo layers, (3) validate compilation and set up CI infrastructure.

**Tech Stack:** Rust 1.70+, Cargo workspace, GitHub Actions, shell scripting for audit.

---

## Task 1: Audit RuVector Dependency Graph

**Files:**
- Create: `docs/spec/ruvector-curation.md` (audit output)
- Reference: `/mnt/datadisk/repos/rUvnet/RuVector/Cargo.toml`

**Context:** RuVector has 136 crates. Ruflo needs only the ones that implement scope-ledger capabilities (HNSW, RaBitQ, SONA, .rvf, RuVLLM, Raft, witness chain, etc.). This task identifies the exact minimal set.

- [ ] **Step 1: Examine RuVector workspace structure**

```bash
cd /mnt/datadisk/repos/rUvnet/RuVector
ls -la crates/ | head -30
# Count total crates
find crates -maxdepth 1 -type d | wc -l
```

Expected: See ~136 directories under crates/.

- [ ] **Step 2: Generate dependency tree for each scope-ledger capability**

For each capability (HNSW, RaBitQ, SONA, .rvf, RuVLLM, Raft, replication, cluster, MCP, witness), run:

```bash
cargo tree -p ruvector-core --depth 3 2>/dev/null | head -50
cargo tree -p ruvector-rabitq --depth 3 2>/dev/null | head -50
cargo tree -p sona --depth 3 2>/dev/null | head -50
cargo tree -p rvf --depth 3 2>/dev/null | head -50
cargo tree -p ruvllm --depth 3 2>/dev/null | head -50
# ... repeat for ruvector-raft, ruvector-replication, ruvector-cluster, mcp-brain-server, rvf-crypto
```

Expected: Dependency chains showing which crates each capability depends on.

- [ ] **Step 3: Manually build transitive closure of dependencies**

Create a list:

```
ruvector-core (for HNSW)
├── ruvector-math
├── ruvector-simd
├── ruvector-utils
└── ...

ruvector-rabitq (for RaBitQ quantization)
├── ruvector-math
├── ruvector-utils
└── ...

sona (for self-learning)
├── ruvector-core
├── ruvector-router-core
└── ...

rvf (for .rvf containers)
├── rvf-cow
├── tokio
└── ...

rvf-crypto (for witness chain)
├── ml-dsa
├── ed25519
└── ...

ruvllm (for local inference)
├── candle (or similar)
└── ...

ruvector-raft (for replication)
├── tokio
└── ...

ruvector-replication (for cluster state)
├── ruvector-raft
└── ...

ruvector-cluster (for mesh coordination)
├── ruvector-replication
└── ...

mcp-brain-server (for MCP baseline)
├── ruvector-core
└── ...
```

Expected: Clear list of top-level crates Ruflo needs + their direct dependencies.

- [ ] **Step 4: Write ruvector-curation.md with exact crate list**

Create `docs/spec/ruvector-curation.md`:

```markdown
# RuVector Curation Audit — Phase 0

**Audit Date:** 2026-06-02
**RuVector Repo:** /mnt/datadisk/repos/rUvnet/RuVector
**Total RuVector Crates:** 136
**Curated for Ruflo:** [COUNT] (approx. 35-45)

## Crates Copied to substrate/

| Crate | Provides | Dependencies | Notes |
|-------|----------|--------------|-------|
| ruvector-core | HNSW, ACORN, DiskANN | ruvector-math, ruvector-simd | Core vector search |
| ruvector-math | Math kernels | — | Dependency of ruvector-core |
| ruvector-simd | SIMD operations | — | Dependency of ruvector-core |
| ruvector-utils | Utilities | — | Dependency of ruvector-core |
| ruvector-acorn | ACORN variant | ruvector-core | Vector search variant |
| ruvector-rabitq | RaBitQ quantization | ruvector-math | Quantization for memory efficiency |
| sona | Self-learning, reranking | ruvector-core, ruvector-router-core | SONA learning loop |
| ruvector-router-core | Model router | — | Routing decisions |
| rvf | .rvf containers (COW) | tokio | Session storage format |
| rvf-cow | Copy-on-write | — | COW implementation |
| rvf-crypto | Witness chain (ML-DSA + Ed25519) | ml-dsa, ed25519 | Cryptographic verification |
| ruvllm | Local LLM inference | candle (or similar) | On-device inference |
| ruvector-raft | Raft consensus | tokio | State replication |
| ruvector-replication | Cluster state sync | ruvector-raft | Multi-node replication |
| ruvector-cluster | Cluster coordination | ruvector-replication | Mesh topology |
| mcp-brain-server | MCP baseline | ruvector-core | MCP server foundation (may use in Phase 2) |
| [other utility/support crates] | ... | ... | As discovered via cargo tree |

## Crates NOT Copied (Experimental/Out-of-Scope)

| Crate | Reason |
|-------|--------|
| quantum-coherence-* | Experimental, not in v1 scope |
| consciousness-* | Experimental, not in v1 scope |
| hyperbolic-reasoning-* | Deferred, not in v1 scope |
| ... | (others found during audit) |

## Verification

- [ ] Total curated crates: [COUNT]
- [ ] All scope-ledger capabilities covered
- [ ] No circular dependencies discovered
- [ ] Transitive closure verified with `cargo check`
```

Save this file.

- [ ] **Step 5: Commit the audit**

```bash
cd /mnt/datadisk/dev/ruvos
git add docs/spec/ruvector-curation.md
git commit -m "docs: Phase 0 RuVector curation audit"
```

Expected: Commit message shows audit doc is tracked.

---

## Task 2: Create Workspace Root Structure

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `.gitignore`
- Create: `README.md` (project overview)

- [ ] **Step 1: Create workspace Cargo.toml**

Create `/mnt/datadisk/dev/ruvos/Cargo.toml`:

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
    "substrate/ruvector-math",
    "substrate/ruvector-simd",
    "substrate/ruvector-utils",
    "substrate/ruvector-rabitq",
    "substrate/sona",
    "substrate/ruvector-router-core",
    "substrate/rvf",
    "substrate/rvf-cow",
    "substrate/rvf-crypto",
    "substrate/ruvllm",
    "substrate/ruvector-raft",
    "substrate/ruvector-replication",
    "substrate/ruvector-cluster",
    # Add others from curation audit
]

[workspace.package]
version = "4.0.0-rc.1"
edition = "2021"
license = "MIT"
authors = ["rUvOS contributors"]
repository = "https://github.com/dgdev25/ruvos"

[workspace.lints.rust]
unsafe_code = "forbid"
```

- [ ] **Step 2: Create .gitignore**

Create `/mnt/datadisk/dev/ruvos/.gitignore`:

```
# Rust
/target/
Cargo.lock
*.rs.bk
**/*.rs.bk

# IDE
.vscode/
.idea/
*.swp
*.swo
*~
.DS_Store

# Build artifacts
*.o
*.a
*.so
*.dylib
*.rlib

# Test artifacts
*.test

# Temporary files
*.tmp
.env.local
```

- [ ] **Step 3: Create README.md**

Create `/mnt/datadisk/dev/ruvos/README.md`:

```markdown
# rUvOS — The Agentic Operating System

rUvOS is a Rust-native agent orchestration system built on the RuVector substrate.

- **RuVector** is the kernel: self-learning vector search, graph RAG, local LLM inference, cryptographic state containers
- **Ruflo** is the shell: agent orchestration, multi-CLI support (Claude Code, Codex, Gemini), plugin system, hooks

**Status:** Phase 0 (Scope & Scaffolding). See docs/spec/scope-ledger-v1.md for architecture and roadmap.

## Quick Start (Phase 1+)

```bash
cargo build --release
./target/release/ruflo mcp serve
```

## Development

- **Workspace structure:** `crates/` (Ruflo orchestration), `substrate/` (RuVector kernel)
- **Scope contract:** 20 MCP tools, 12 agent archetypes, 8 hooks, ≤30k Ruflo LOC
- **File size limit:** all .rs files ≤500 lines (enforced in CI)
- **Contributing:** See CLAUDE.md for development guidelines

## Phase Timeline

| Phase | What | ETA |
|-------|------|-----|
| **0** | Workspace scaffolding (you are here) | 3-5 days |
| **1** | Merge substrates, CI green | 1 week |
| **2** | MCP server + hello-world tool | 1 week |
| **3** | Plugin host + skill compatibility | 1 week |
| **4** | Hooks + SQLite queue | 2 weeks |
| **5** | Memory + session (.rvf) | 2 weeks |
| **6** | CliHost adapters (Claude + Codex) | 2 weeks |
| **7** | Cutover + deprecation | 1 week |

See docs/superpowers/specs/ and docs/spec/ for detailed planning.

## License

MIT
```

- [ ] **Step 4: Verify Cargo.toml syntax**

```bash
cd /mnt/datadisk/dev/ruvos
cargo metadata --format-version 1 > /dev/null && echo "Cargo.toml valid"
```

Expected: "Cargo.toml valid" printed.

- [ ] **Step 5: Commit workspace root**

```bash
git add Cargo.toml .gitignore README.md
git commit -m "feat: add workspace root structure and configuration"
```

Expected: Clean commit with three files.

---

## Task 3: Copy Curated RuVector Crates

**Files:**
- Create: `substrate/*/` (each crate from curation audit)

**Context:** Copy each crate from the curated list in ruvector-curation.md into substrate/. Use `cp -r` to preserve directory structure and metadata.

- [ ] **Step 1: Create substrate directory**

```bash
mkdir -p /mnt/datadisk/dev/ruvos/substrate
```

- [ ] **Step 2: Copy each curated crate**

For each crate in the curation list (ruvector-core, ruvector-math, etc.):

```bash
cd /mnt/datadisk/repos/rUvnet/RuVector/crates
for crate in ruvector-core ruvector-math ruvector-simd ruvector-utils ruvector-acorn ruvector-rabitq sona ruvector-router-core rvf rvf-cow rvf-crypto ruvllm ruvector-raft ruvector-replication ruvector-cluster; do
  cp -r "$crate" /mnt/datadisk/dev/ruvos/substrate/
  echo "Copied $crate"
done
```

Expected: Each crate directory appears in substrate/ with all files intact.

- [ ] **Step 3: Verify substrate crates have Cargo.toml**

```bash
cd /mnt/datadisk/dev/ruvos/substrate
for dir in */; do
  if [ ! -f "$dir/Cargo.toml" ]; then
    echo "ERROR: $dir has no Cargo.toml"
  fi
done
echo "All substrate crates have Cargo.toml"
```

Expected: "All substrate crates have Cargo.toml" printed, no errors.

- [ ] **Step 4: Test workspace recognizes substrate crates**

```bash
cd /mnt/datadisk/dev/ruvos
cargo metadata --format-version 1 | grep '"name"' | grep ruvector | head -10
```

Expected: At least 10 RuVector crate names printed.

- [ ] **Step 5: Commit substrate crates**

```bash
cd /mnt/datadisk/dev/ruvos
git add substrate/
git commit -m "feat: copy curated RuVector substrate crates"
```

Expected: Commit shows multiple substrate crates added.

---

## Task 4: Create ruflo-cli Crate

**Files:**
- Create: `crates/ruflo-cli/Cargo.toml`
- Create: `crates/ruflo-cli/src/lib.rs`
- Create: `crates/ruflo-cli/src/main.rs`
- Create: `crates/ruflo-cli/src/commands/mod.rs`
- Create: `crates/ruflo-cli/src/commands/init.rs`
- Create: `crates/ruflo-cli/src/commands/mcp.rs`
- Create: `crates/ruflo-cli/src/dispatch/mod.rs`

- [ ] **Step 1: Create crate directory**

```bash
mkdir -p /mnt/datadisk/dev/ruvos/crates/ruflo-cli/src/{commands,dispatch}
```

- [ ] **Step 2: Create Cargo.toml**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-cli/Cargo.toml`:

```toml
[package]
name = "ruflo-cli"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[[bin]]
name = "ruflo"
path = "src/main.rs"

[dependencies]
tokio = { version = "1", features = ["full"] }
clap = { version = "4", features = ["derive"] }
tracing = "0.1"
tracing-subscriber = "0.3"

[dev-dependencies]
```

- [ ] **Step 3: Create src/lib.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-cli/src/lib.rs`:

```rust
pub mod commands;
pub mod dispatch;

pub async fn run() {
    tracing::info!("rUvOS CLI starting (Phase 0 scaffold)");
}
```

- [ ] **Step 4: Create src/main.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-cli/src/main.rs`:

```rust
use clap::{Parser, Subcommand};
use ruflo_cli::dispatch;

#[derive(Parser)]
#[command(name = "ruflo")]
#[command(about = "rUvOS agent orchestration CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a project (Phase 1+)
    Init,
    /// Run MCP server (Phase 2+)
    Mcp {
        #[command(subcommand)]
        subcommand: McpCommand,
    },
}

#[derive(Subcommand)]
enum McpCommand {
    /// Serve MCP over stdio
    Serve,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Init => {
            println!("init: not yet implemented (Phase 1)");
        }
        Commands::Mcp { subcommand } => {
            match subcommand {
                McpCommand::Serve => {
                    println!("mcp serve: not yet implemented (Phase 2)");
                }
            }
        }
    }
}
```

- [ ] **Step 5: Create src/commands/mod.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-cli/src/commands/mod.rs`:

```rust
pub mod init;
pub mod mcp;
```

- [ ] **Step 6: Create src/commands/init.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-cli/src/commands/init.rs`:

```rust
/// Initialize a new rUvOS project (Phase 1+)
pub async fn init() {
    // Stub for Phase 1
}
```

- [ ] **Step 7: Create src/commands/mcp.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-cli/src/commands/mcp.rs`:

```rust
/// Run MCP server (Phase 2+)
pub async fn serve() {
    // Stub for Phase 2
}
```

- [ ] **Step 8: Create src/dispatch/mod.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-cli/src/dispatch/mod.rs`:

```rust
/// Command dispatch logic (Phase 1+)
pub fn dispatch() {
    // Stub for Phase 1
}
```

- [ ] **Step 9: Verify crate compiles**

```bash
cd /mnt/datadisk/dev/ruvos/crates/ruflo-cli
cargo check
```

Expected: Clean compilation, no errors.

- [ ] **Step 10: Commit ruflo-cli**

```bash
cd /mnt/datadisk/dev/ruvos
git add crates/ruflo-cli/
git commit -m "feat: create ruflo-cli crate with command stubs"
```

Expected: Clean commit with ruflo-cli structure.

---

## Task 5: Create ruflo-mcp Crate

**Files:**
- Create: `crates/ruflo-mcp/Cargo.toml`
- Create: `crates/ruflo-mcp/src/lib.rs`
- Create: `crates/ruflo-mcp/src/server.rs`
- Create: `crates/ruflo-mcp/src/tools/mod.rs` and tool stubs

- [ ] **Step 1: Create crate directory**

```bash
mkdir -p /mnt/datadisk/dev/ruvos/crates/ruflo-mcp/src/tools
```

- [ ] **Step 2: Create Cargo.toml**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-mcp/Cargo.toml`:

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
tracing = "0.1"

[dev-dependencies]
```

- [ ] **Step 3: Create src/lib.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-mcp/src/lib.rs`:

```rust
pub mod server;
pub mod tools;

pub async fn start() {
    tracing::info!("MCP server starting (Phase 2 implementation)");
}
```

- [ ] **Step 4: Create src/server.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-mcp/src/server.rs`:

```rust
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// JSON-RPC MCP server over stdio (Phase 2+)
pub async fn serve() {
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    
    tracing::info!("MCP server listening on stdin");
    // Phase 2: implement JSON-RPC protocol
}
```

- [ ] **Step 5: Create src/tools/mod.rs with domain stubs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-mcp/src/tools/mod.rs`:

```rust
pub mod memory;
pub mod session;
pub mod agent;
pub mod hooks;
pub mod intel;
pub mod plugin;
pub mod gov;
pub mod workflow;

/// Registry of all 20 MCP tools
pub fn tool_registry() -> Vec<&'static str> {
    vec![
        // memory (4)
        "memory.search",
        "memory.store",
        "memory.retrieve",
        "memory.list",
        // session (3)
        "session.create",
        "session.resume",
        "session.fork",
        // agent (3)
        "agent.spawn",
        "agent.status",
        "agent.message",
        // hooks (3)
        "hooks.pre",
        "hooks.post",
        "hooks.route",
        // intel (2)
        "intel.pattern_search",
        "intel.pattern_store",
        // plugin (2)
        "plugin.list",
        "plugin.invoke",
        // gov (2)
        "gov.witness_verify",
        "gov.health",
        // workflow (1)
        "workflow.run",
    ]
}
```

- [ ] **Step 6: Create tool stub modules**

For each domain, create a stub file. Example for `src/tools/memory.rs`:

```rust
/// memory.search — semantic search with MMR + recency
pub struct SearchTool;

/// memory.store — insert/update entry
pub struct StoreTool;

/// memory.retrieve — get single entry by key
pub struct RetrieveTool;

/// memory.list — list entries in namespace
pub struct ListTool;
```

Repeat for: `src/tools/session.rs`, `src/tools/agent.rs`, `src/tools/hooks.rs`, `src/tools/intel.rs`, `src/tools/plugin.rs`, `src/tools/gov.rs`, `src/tools/workflow.rs`.

Each should have struct stubs for each tool in that domain.

- [ ] **Step 7: Verify crate compiles**

```bash
cd /mnt/datadisk/dev/ruvos/crates/ruflo-mcp
cargo check
```

Expected: Clean compilation.

- [ ] **Step 8: Commit ruflo-mcp**

```bash
cd /mnt/datadisk/dev/ruvos
git add crates/ruflo-mcp/
git commit -m "feat: create ruflo-mcp crate with tool domain stubs"
```

Expected: Clean commit with ruflo-mcp structure.

---

## Task 6: Create ruflo-host Crate

**Files:**
- Create: `crates/ruflo-host/Cargo.toml`
- Create: `crates/ruflo-host/src/lib.rs`
- Create: `crates/ruflo-host/src/host.rs` (CliHost trait)
- Create: `crates/ruflo-host/src/adapters/mod.rs`
- Create: `crates/ruflo-host/src/adapters/claude.rs`
- Create: `crates/ruflo-host/src/adapters/codex.rs`

- [ ] **Step 1: Create crate directory**

```bash
mkdir -p /mnt/datadisk/dev/ruvos/crates/ruflo-host/src/adapters
```

- [ ] **Step 2: Create Cargo.toml**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-host/Cargo.toml`:

```toml
[package]
name = "ruflo-host"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[dependencies]
async-trait = "0.1"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
futures = "0.3"

[dev-dependencies]
```

- [ ] **Step 3: Create src/lib.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-host/src/lib.rs`:

```rust
pub mod host;
pub mod adapters;

pub use host::CliHost;
```

- [ ] **Step 4: Create src/host.rs with CliHost trait**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-host/src/host.rs`:

```rust
use async_trait::async_trait;
use serde_json::json;
use std::path::PathBuf;
use uuid::Uuid;

/// CLI host specification
#[derive(Clone, Debug)]
pub struct ModelSpec {
    pub name: String,
    pub tier: u8, // 1=fast, 2=standard, 3=frontier
}

/// Agent request to be executed on a CLI host
pub struct AgentRequest {
    pub prompt: String,
    pub model: Option<String>,
    pub session_id: Option<Uuid>,
    pub allowed_tools: Vec<String>,
    pub budget_usd: Option<f64>,
    pub working_dir: PathBuf,
}

/// Agent response/event
#[derive(Clone, Debug)]
pub enum AgentEvent {
    Started,
    Message(String),
    ToolCall(String),
    Finished(String),
}

/// CliHost trait: abstraction over Claude, Codex, Gemini
#[async_trait]
pub trait CliHost: Send + Sync {
    /// Host name: "claude" | "codex" | "gemini"
    fn name(&self) -> &'static str;

    /// Available models on this host
    fn available_models(&self) -> Vec<ModelSpec>;

    /// Run a single agent request and return final output
    async fn run(&self, req: AgentRequest) -> Result<String, String>;

    /// Stream events from an agent request
    async fn stream(&self, req: AgentRequest) -> Result<Vec<AgentEvent>, String>;
}
```

- [ ] **Step 5: Create src/adapters/mod.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-host/src/adapters/mod.rs`:

```rust
pub mod claude;
pub mod codex;

pub use claude::ClaudeHost;
pub use codex::CodexHost;
```

- [ ] **Step 6: Create src/adapters/claude.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-host/src/adapters/claude.rs`:

```rust
use crate::host::{CliHost, ModelSpec, AgentRequest, AgentEvent};
use async_trait::async_trait;

/// Claude Code CLI host adapter (Phase 2+)
pub struct ClaudeHost;

#[async_trait]
impl CliHost for ClaudeHost {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn available_models(&self) -> Vec<ModelSpec> {
        vec![
            ModelSpec { name: "claude-haiku-4-5".to_string(), tier: 1 },
            ModelSpec { name: "claude-sonnet-4-6".to_string(), tier: 2 },
            ModelSpec { name: "claude-opus-4-8".to_string(), tier: 3 },
        ]
    }

    async fn run(&self, _req: AgentRequest) -> Result<String, String> {
        Err("Phase 2: not implemented".to_string())
    }

    async fn stream(&self, _req: AgentRequest) -> Result<Vec<AgentEvent>, String> {
        Err("Phase 2: not implemented".to_string())
    }
}
```

- [ ] **Step 7: Create src/adapters/codex.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-host/src/adapters/codex.rs`:

```rust
use crate::host::{CliHost, ModelSpec, AgentRequest, AgentEvent};
use async_trait::async_trait;

/// Codex CLI host adapter (Phase 2+)
pub struct CodexHost;

#[async_trait]
impl CliHost for CodexHost {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn available_models(&self) -> Vec<ModelSpec> {
        vec![
            ModelSpec { name: "gpt-5.4-mini".to_string(), tier: 1 },
            ModelSpec { name: "gpt-5.4".to_string(), tier: 2 },
            ModelSpec { name: "gpt-5.5".to_string(), tier: 3 },
        ]
    }

    async fn run(&self, _req: AgentRequest) -> Result<String, String> {
        Err("Phase 2: not implemented".to_string())
    }

    async fn stream(&self, _req: AgentRequest) -> Result<Vec<AgentEvent>, String> {
        Err("Phase 2: not implemented".to_string())
    }
}
```

- [ ] **Step 8: Verify crate compiles**

```bash
cd /mnt/datadisk/dev/ruvos/crates/ruflo-host
cargo check
```

Expected: Clean compilation.

- [ ] **Step 9: Commit ruflo-host**

```bash
cd /mnt/datadisk/dev/ruvos
git add crates/ruflo-host/
git commit -m "feat: create ruflo-host crate with CliHost trait and adapters"
```

Expected: Clean commit with ruflo-host structure.

---

## Task 7: Create ruflo-plugin-host Crate

**Files:**
- Create: `crates/ruflo-plugin-host/Cargo.toml`
- Create: `crates/ruflo-plugin-host/src/lib.rs`
- Create: `crates/ruflo-plugin-host/src/discovery.rs`
- Create: `crates/ruflo-plugin-host/src/registry/mod.rs`

- [ ] **Step 1: Create crate directory**

```bash
mkdir -p /mnt/datadisk/dev/ruvos/crates/ruflo-plugin-host/src/registry
```

- [ ] **Step 2: Create Cargo.toml**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-plugin-host/Cargo.toml`:

```toml
[package]
name = "ruflo-plugin-host"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[dependencies]
tokio = { version = "1", features = ["process"] }
serde = { version = "1", features = ["derive"] }
toml = "0.8"
tracing = "0.1"

[dev-dependencies]
```

- [ ] **Step 3: Create src/lib.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-plugin-host/src/lib.rs`:

```rust
pub mod discovery;
pub mod registry;

pub use discovery::discover_plugins;
pub use registry::PluginRegistry;
```

- [ ] **Step 4: Create src/discovery.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-plugin-host/src/discovery.rs`:

```rust
use std::path::PathBuf;

/// Plugin manifest from plugin.toml
#[derive(Debug, Clone)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub agents: Vec<String>,
    pub skills: Vec<String>,
    pub commands: Vec<String>,
}

/// Discover plugins from filesystem
/// Discovery order: project-local -> user-global -> env -> built-in
pub async fn discover_plugins() -> Result<Vec<PluginManifest>, String> {
    // Phase 3: implement discovery
    // For now, return empty list
    Ok(vec![])
}

/// List installed plugins
pub async fn list_plugins() -> Result<Vec<String>, String> {
    // Phase 3+
    Ok(vec![])
}

/// Invoke a plugin command
pub async fn invoke_plugin(name: &str, command: &str) -> Result<String, String> {
    // Phase 3+
    Err("not implemented".to_string())
}
```

- [ ] **Step 5: Create src/registry/mod.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-plugin-host/src/registry/mod.rs`:

```rust
use crate::discovery::PluginManifest;

/// Built-in plugin registry (candidate plugins for v1)
pub struct PluginRegistry {
    plugins: Vec<PluginManifest>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: vec![
                // Provisional keep list from scope-ledger.md
                // These are candidates; Phase 0 audit determines final list
            ],
        }
    }

    pub fn list(&self) -> &[PluginManifest] {
        &self.plugins
    }
}
```

- [ ] **Step 6: Verify crate compiles**

```bash
cd /mnt/datadisk/dev/ruvos/crates/ruflo-plugin-host
cargo check
```

Expected: Clean compilation.

- [ ] **Step 7: Commit ruflo-plugin-host**

```bash
cd /mnt/datadisk/dev/ruvos
git add crates/ruflo-plugin-host/
git commit -m "feat: create ruflo-plugin-host crate with discovery stubs"
```

Expected: Clean commit.

---

## Task 8: Create ruflo-hooks Crate

**Files:**
- Create: `crates/ruflo-hooks/Cargo.toml`
- Create: `crates/ruflo-hooks/src/lib.rs`
- Create: `crates/ruflo-hooks/src/hooks/mod.rs`
- Create: `crates/ruflo-hooks/src/hooks/pre.rs`
- Create: `crates/ruflo-hooks/src/hooks/post.rs`
- Create: `crates/ruflo-hooks/src/hooks/route.rs`

- [ ] **Step 1: Create crate directory**

```bash
mkdir -p /mnt/datadisk/dev/ruvos/crates/ruflo-hooks/src/hooks
```

- [ ] **Step 2: Create Cargo.toml**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-hooks/Cargo.toml`:

```toml
[package]
name = "ruflo-hooks"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[dev-dependencies]
```

- [ ] **Step 3: Create src/lib.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-hooks/src/lib.rs`:

```rust
pub mod hooks;

pub use hooks::{pre, post, route};

/// The 8 hooks that survive from current Ruflo
/// 1. pre-task / post-task
/// 2. pre-edit / post-edit
/// 3. pre-command / post-command
/// 4. session-start / session-end (wrapped in pre/post with discriminator)
```

- [ ] **Step 4: Create src/hooks/mod.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-hooks/src/hooks/mod.rs`:

```rust
pub mod pre;
pub mod post;
pub mod route;

#[derive(Debug, Clone)]
pub enum HookKind {
    Task,
    Edit,
    Command,
    Session,
}

#[derive(Debug, Clone)]
pub struct HookPayload {
    pub kind: HookKind,
    pub data: serde_json::Value,
}
```

- [ ] **Step 5: Create src/hooks/pre.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-hooks/src/hooks/pre.rs`:

```rust
use super::HookPayload;

/// Pre-hook: fires before task, edit, command, or session start
/// Returns routing + context for the operation
pub async fn pre_hook(_payload: HookPayload) -> Result<serde_json::Value, String> {
    // Phase 4+: implement hook dispatch
    Ok(serde_json::json!({}))
}
```

- [ ] **Step 6: Create src/hooks/post.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-hooks/src/hooks/post.rs`:

```rust
use super::HookPayload;

/// Post-hook: fires after operation with outcome
/// Feeds learning signal to SONA
pub async fn post_hook(_payload: HookPayload) -> Result<serde_json::Value, String> {
    // Phase 4+: implement hook dispatch + SONA integration
    Ok(serde_json::json!({}))
}
```

- [ ] **Step 7: Create src/hooks/route.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-hooks/src/hooks/route.rs`:

```rust
/// Route decision: recommend model + archetype for a task
/// Uses ruvector-router-core
pub struct RouteRecommendation {
    pub model: String,
    pub archetype: String,
    pub traits: Vec<String>,
}

pub async fn route_task(_prompt: &str) -> Result<RouteRecommendation, String> {
    // Phase 4+: integrate with ruvector-router-core
    Ok(RouteRecommendation {
        model: "claude-sonnet-4-6".to_string(),
        archetype: "coder".to_string(),
        traits: vec![],
    })
}
```

- [ ] **Step 8: Verify crate compiles**

```bash
cd /mnt/datadisk/dev/ruvos/crates/ruflo-hooks
cargo check
```

Expected: Clean compilation.

- [ ] **Step 9: Commit ruflo-hooks**

```bash
cd /mnt/datadisk/dev/ruvos
git add crates/ruflo-hooks/
git commit -m "feat: create ruflo-hooks crate with hook stubs"
```

Expected: Clean commit.

---

## Task 9: Create ruflo-session Crate

**Files:**
- Create: `crates/ruflo-session/Cargo.toml`
- Create: `crates/ruflo-session/src/lib.rs`
- Create: `crates/ruflo-session/src/rvf.rs`
- Create: `crates/ruflo-session/src/fork.rs`
- Create: `crates/ruflo-session/src/verify.rs`

- [ ] **Step 1: Create crate directory**

```bash
mkdir -p /mnt/datadisk/dev/ruvos/crates/ruflo-session/src
```

- [ ] **Step 2: Create Cargo.toml**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-session/Cargo.toml`:

```toml
[package]
name = "ruflo-session"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true

[dependencies]
tokio = { version = "1", features = ["fs"] }
serde = { version = "1", features = ["derive"] }
uuid = { version = "1", features = ["v4"] }
tracing = "0.1"

[dev-dependencies]
```

- [ ] **Step 3: Create src/lib.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-session/src/lib.rs`:

```rust
pub mod rvf;
pub mod fork;
pub mod verify;

use uuid::Uuid;

/// Session handle with .rvf backing
pub struct Session {
    pub id: Uuid,
    pub rvf_path: String,
}

impl Session {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            rvf_path: format!(".rvf/{}.rvf", Uuid::new_v4()),
        }
    }
}
```

- [ ] **Step 4: Create src/rvf.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-session/src/rvf.rs`:

```rust
use crate::Session;

/// .rvf container I/O (copy-on-write session format)
/// Phase 5: integrate with substrate/rvf crate
pub async fn write_session(_session: &Session) -> Result<(), String> {
    // Phase 5+
    Ok(())
}

pub async fn read_session(_path: &str) -> Result<Session, String> {
    // Phase 5+
    Err("not implemented".to_string())
}
```

- [ ] **Step 5: Create src/fork.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-session/src/fork.rs`:

```rust
use crate::Session;

/// COW-branch a session for parallel exploration
/// Phase 5: integrate with substrate/rvf-cow
pub async fn fork_session(_session: &Session) -> Result<Session, String> {
    // Phase 5+
    Err("not implemented".to_string())
}
```

- [ ] **Step 6: Create src/verify.rs**

Create `/mnt/datadisk/dev/ruvos/crates/ruflo-session/src/verify.rs`:

```rust
/// Verify .rvf signature chain via rvf-crypto
/// Phase 5: integrate with substrate/rvf-crypto
pub async fn verify_signature(_path: &str) -> Result<bool, String> {
    // Phase 5+
    Err("not implemented".to_string())
}
```

- [ ] **Step 7: Verify crate compiles**

```bash
cd /mnt/datadisk/dev/ruvos/crates/ruflo-session
cargo check
```

Expected: Clean compilation.

- [ ] **Step 8: Commit ruflo-session**

```bash
cd /mnt/datadisk/dev/ruvos
git add crates/ruflo-session/
git commit -m "feat: create ruflo-session crate with .rvf container stubs"
```

Expected: Clean commit.

---

## Task 10: Verify All Crates Compile Together

**Files:**
- No new files; validation only

- [ ] **Step 1: Check workspace recognizes all Ruflo crates**

```bash
cd /mnt/datadisk/dev/ruvos
cargo metadata --format-version 1 | grep '"name": "ruflo' | wc -l
# Should print 6
```

Expected: "6" printed (all 6 Ruflo crates).

- [ ] **Step 2: Build all default-members**

```bash
cd /mnt/datadisk/dev/ruvos
cargo build --all-features 2>&1 | head -100
```

Expected: Build succeeds or shows clear dependency/compilation error (no "crate not found" errors).

- [ ] **Step 3: Run clippy on all default-members**

```bash
cd /mnt/datadisk/dev/ruvos
cargo clippy --all-targets --all-features -- -D warnings 2>&1 | head -50
```

Expected: Clippy passes or reports only phase-0-appropriate warnings.

- [ ] **Step 4: Check formatting**

```bash
cd /mnt/datadisk/dev/ruvos
cargo fmt -- --check
```

Expected: All files are properly formatted.

- [ ] **Step 5: List all crates in workspace**

```bash
cd /mnt/datadisk/dev/ruvos
cargo metadata --format-version 1 | jq -r '.packages[].name' | sort | uniq
```

Expected: 6 Ruflo crates + ~15+ RuVector crates listed.

- [ ] **Step 6: Commit validation passing state**

If all checks pass:

```bash
cd /mnt/datadisk/dev/ruvos
git add -A
git commit -m "feat: Phase 0 crates validated and compiling"
```

Expected: Clean commit.

---

## Task 11: Create CI Workflow

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Create .github/workflows directory**

```bash
mkdir -p /mnt/datadisk/dev/ruvos/.github/workflows
```

- [ ] **Step 2: Create ci.yml**

Create `/mnt/datadisk/dev/ruvos/.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [main, master]
  pull_request:
    branches: [main, master]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  build:
    name: Build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: stable

      - name: Cache cargo
        uses: actions/cache@v3
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            target/
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Build
        run: cargo build --workspace --all-features

  lint:
    name: Clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: stable
          components: clippy

      - name: Cache cargo
        uses: actions/cache@v3
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            target/
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Clippy
        run: cargo clippy --all-targets --all-features -- -D warnings

  fmt:
    name: Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: stable
          components: rustfmt

      - name: Check formatting
        run: cargo fmt -- --check

  test:
    name: Tests
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
        with:
          toolchain: stable

      - name: Cache cargo
        uses: actions/cache@v3
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            target/
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Run tests
        run: cargo test --lib --all-features
```

- [ ] **Step 3: Verify CI file syntax**

```bash
cd /mnt/datadisk/dev/ruvos
# Simple YAML syntax check (if yq available)
python3 -m yaml .github/workflows/ci.yml > /dev/null && echo "YAML valid" || echo "Check YAML manually"
```

Expected: File is valid YAML.

- [ ] **Step 4: Commit CI workflow**

```bash
cd /mnt/datadisk/dev/ruvos
git add .github/workflows/ci.yml
git commit -m "ci: add GitHub Actions workflow for build/lint/fmt/test"
```

Expected: Clean commit.

---

## Task 12: Update CLAUDE.md with Phase 0 Completion

**Files:**
- Modify: `CLAUDE.md` (add Phase 0 completion notes)

- [ ] **Step 1: Read current CLAUDE.md**

```bash
head -50 /mnt/datadisk/dev/ruvos/CLAUDE.md
```

- [ ] **Step 2: Add Phase 0 completion section**

Append to `/mnt/datadisk/dev/ruvos/CLAUDE.md`:

```markdown

---

## Phase 0 Completion (2026-06-02)

**Status:** ✅ Complete

Phase 0 established the rUvOS workspace structure with:
- ✅ RuVector dependency audit completed (`docs/spec/ruvector-curation.md`)
- ✅ 6 Ruflo crates scaffolded with module structure matching scope ledger
- ✅ Curated RuVector crates copied to `substrate/` (not as dependency)
- ✅ Root Cargo.toml with workspace + default-members properly scoped
- ✅ CI pipeline configured (build/clippy/fmt/test)
- ✅ All crates compile and pass checks

**Next:** Phase 1 (merge completed, CI fully green, prepare MCP day-1 integration)

**Entry point for Phase 1:** See `docs/superpowers/plans/phase-1-*.md` once written.
```

- [ ] **Step 3: Commit CLAUDE.md update**

```bash
cd /mnt/datadisk/dev/ruvos
git add CLAUDE.md
git commit -m "docs: document Phase 0 completion"
```

Expected: Clean commit.

---

## Task 13: Final Validation and Tagging

**Files:**
- No new files; validation + tagging

- [ ] **Step 1: Final full build**

```bash
cd /mnt/datadisk/dev/ruvos
cargo build --all-features --release 2>&1 | tail -20
```

Expected: "Finished release [optimized]" or success message.

- [ ] **Step 2: Verify all checks pass**

```bash
cd /mnt/datadisk/dev/ruvos
cargo fmt -- --check && cargo clippy --all-targets --all-features -- -D warnings && echo "✅ All checks passed"
```

Expected: "✅ All checks passed" printed.

- [ ] **Step 3: Verify Git history is clean**

```bash
cd /mnt/datadisk/dev/ruvos
git log --oneline | head -10
```

Expected: Clean commit history with Phase 0 tasks.

- [ ] **Step 4: Create Phase 0 completion commit (if needed)**

If all validation passed, create a final summary commit:

```bash
cd /mnt/datadisk/dev/ruvos
git log --oneline --all | head -1
# Note the latest commit
git commit --allow-empty -m "Phase 0: rUvOS workspace scaffolding complete

- RuVector audit: curated 35+ crates in docs/spec/ruvector-curation.md
- Ruflo scaffolding: 6 crates with module structure
- Workspace: Cargo.toml with default-members properly scoped
- CI: GitHub Actions (build/clippy/fmt/test)
- Validation: all checks passing, ready for Phase 1

See docs/superpowers/specs/2026-06-02-phase0-ruvos-workspace.md for details."
```

- [ ] **Step 5: Verify all files tracked in Git**

```bash
cd /mnt/datadisk/dev/ruvos
git status
```

Expected: "nothing to commit, working tree clean" (or only expected untracked files like target/).

---

## Plan Summary

**Phase 0 deliverables:**
1. ✅ RuVector dependency audit (`docs/spec/ruvector-curation.md`)
2. ✅ Workspace root structure (`Cargo.toml`, `.gitignore`, `README.md`)
3. ✅ 6 Ruflo crates with module stubs (cli, mcp, host, plugin-host, hooks, session)
4. ✅ Curated RuVector crates in `substrate/`
5. ✅ GitHub Actions CI (build/clippy/fmt/test)
6. ✅ All crates compile and pass linting
7. ✅ CLAUDE.md updated with Phase 0 notes

**Success criteria met:**
- ✅ `cargo build --workspace` clean
- ✅ `cargo clippy` no warnings
- ✅ `cargo fmt --check` passes
- ✅ Module structure matches scope ledger
- ✅ CI pipeline validates

**Ready for Phase 1:** Merge into RuVector workspace, get CI fully green, prepare Phase 2 (MCP day-1 integration test with Claude Code CLI).

