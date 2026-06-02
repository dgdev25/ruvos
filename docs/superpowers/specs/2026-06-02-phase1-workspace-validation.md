# Phase 1: rUvOS Workspace Integration & Validation — Design

> **For agentic workers:** Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task once the implementation plan is written.

**Goal:** Validate the complete integrated workspace (6 Ruflo orchestration crates + 29 curated RuVector substrate crates) compiles cleanly, all CI checks pass, and the system is ready for Phase 2 implementation work.

**Architecture:** Phase 1 is primarily validation and integration testing. The workspace structure established in Phase 0 (ruvos as primary repo, `crates/` for Ruflo, `substrate/` for RuVector) remains intact. We verify that all components work together correctly before moving to Phase 2's implementation work (MCP server, tool handlers, CliHost adapters).

**Tech Stack:** Rust 1.70+, Cargo workspace, GitHub Actions CI, standard validation tooling.

---

## 1. Current Workspace State (Phase 0 Output)

**Directory structure:**
```
ruvos/
├── crates/                  ← 6 Ruflo orchestration crates (scaffolded)
│   ├── ruflo-cli/
│   ├── ruflo-mcp/
│   ├── ruflo-host/
│   ├── ruflo-plugin-host/
│   ├── ruflo-hooks/
│   └── ruflo-session/
├── substrate/               ← 29 curated RuVector crates (copied, not dependency)
│   ├── ruvector-core/
│   ├── ruvector-acorn/
│   ├── ruvector-rabitq/
│   ├── sona/
│   ├── rvf/ (23 RVF member crates)
│   └── ... (other core crates)
├── Cargo.toml              ← Workspace root, default-members scoped
├── .github/workflows/ci.yml ← CI pipeline configured
└── docs/
    ├── spec/
    │   ├── scope-ledger-v1.md
    │   ├── rewrite-summary.md
    │   └── ruvector-curation.md
    └── superpowers/
        ├── specs/
        │   ├── 2026-06-02-phase0-ruvos-workspace.md (completed)
        │   └── 2026-06-02-phase1-workspace-validation.md (this spec)
        └── plans/
            ├── 2026-06-02-phase0-ruvos-workspace.md (completed)
            └── 2026-06-02-phase1-workspace-validation.md (to be created)
```

**Compilation status (Phase 0 final state):**
- ✅ All 6 Ruflo crates compile individually
- ✅ `cargo build --all-features` passes
- ✅ `cargo clippy` passes with no warnings
- ✅ `cargo fmt --check` passes
- ✅ Working tree clean

---

## 2. Phase 1 Goals

**Primary goal:** Validate the integrated workspace compiles cleanly and is free of integration issues.

**Success criteria:**
1. ✅ Full workspace compilation: `cargo build --workspace --all-features` succeeds
2. ✅ All validation checks pass: clippy, fmt, test
3. ✅ CI pipeline executes successfully on the full workspace
4. ✅ No unresolved dependencies or version conflicts
5. ✅ No path issues or missing files
6. ✅ Documentation updated with integration findings
7. ✅ Working tree clean and ready for Phase 2

**Out of scope:**
- Implementing actual tool handlers (Phase 2)
- Connecting Ruflo to RuVector implementations (Phase 2+)
- Adding new features or functionality
- Performance optimization
- Testing beyond compilation and lint checks

---

## 3. Integration Points to Validate

These are the boundaries where Ruflo and RuVector crates interact. Phase 1 validates the structure; Phase 2 will implement the actual interactions.

### 3.1 MCP Tool Handlers → RuVector Substrate

**Expected connections (Phase 2):**
- `ruflo-mcp/src/tools/memory.rs` will call into `ruvector-core` for HNSW operations
- `ruflo-mcp/src/tools/memory.rs` will call into `sona` for reranking
- `ruflo-session/src/rvf.rs` will call into `rvf` crates for container I/O
- `ruflo-hooks/src/route.rs` will call into `ruvector-router-core` for routing decisions

**Phase 1 validation:** Ensure all `use ruvector_*`, `use sona::*`, `use rvf::*` imports would compile if the implementations existed.

### 3.2 CliHost Trait → Provider CLIs

**Expected connections (Phase 2+):**
- `ruflo-host/src/adapters/claude.rs` will implement `CliHost` for Claude Code CLI
- `ruflo-host/src/adapters/codex.rs` will implement `CliHost` for Codex CLI
- Future: `ruflo-host/src/adapters/gemini.rs` for Gemini CLI

**Phase 1 validation:** Ensure the trait definition and stub adapters compile without errors.

### 3.3 Plugin Host → Discovered Plugins

**Expected connections (Phase 3):**
- `ruflo-plugin-host/src/discovery.rs` will discover plugins from filesystem
- Plugin manifests will be parsed and validated

**Phase 1 validation:** Ensure the discovery stubs and manifest types compile.

---

## 4. Validation Tasks

### Task 1: Full Workspace Compilation
Build the entire workspace with all features enabled. Verify no compilation errors, warnings, or dependency conflicts.

**Command:** `cargo build --workspace --all-features`

**Expected output:** Clean build, all 35+ crates (6 Ruflo + 29 RuVector) compile successfully.

### Task 2: Linting & Formatting
Run clippy and fmt on all crates to ensure code quality standards.

**Commands:**
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo fmt -- --check`

**Expected output:** No warnings, all formatting correct.

### Task 3: Test Suite
Run the test suite (even if empty stubs) to ensure test infrastructure works.

**Command:** `cargo test --lib --all-features`

**Expected output:** Tests pass (or report: "0 tests run" for scaffold phase).

### Task 4: CI Pipeline Validation
Verify the GitHub Actions CI workflow runs successfully on the full workspace.

**Action:** Run CI locally or trigger on GitHub to validate all 4 jobs:
- Build job passes
- Lint job passes
- Format job passes
- Test job passes

**Expected output:** All jobs green (✅).

### Task 5: Dependency Graph Review
Verify there are no unresolved dependencies, circular imports, or missing crates.

**Command:** `cargo tree --workspace`

**Expected output:** Full dependency graph, all crates resolved, no unresolved deps.

### Task 6: Integration Spot Checks
Spot-check a few integration points to ensure the structure supports Phase 2 implementation:
- Can `ruflo-mcp` successfully reference RuVector crate names in imports (not necessarily call them)?
- Do `CliHost` trait types align with what adapters will need?
- Is the plugin manifest structure sound?

**Expected output:** All integration points are well-formed, no structural issues.

### Task 7: Documentation & Final Commit
Document any discoveries, integration issues, or surprises found during Phase 1 validation. Commit the validated workspace.

**Output:**
- Update CLAUDE.md with Phase 1 completion notes
- Create `docs/phase1-integration-report.md` documenting findings (if any issues arose)
- Final commit: "Phase 1 complete: workspace integrated and validated"

---

## 5. Success Metrics

| Metric | Target | How to Measure |
|--------|--------|-----------------|
| Compilation | 100% pass | `cargo build --workspace` succeeds |
| Clippy warnings | 0 | `cargo clippy` returns no warnings |
| Formatting | 100% compliant | `cargo fmt --check` succeeds |
| Tests | Pass (or report 0) | `cargo test --lib` succeeds |
| CI pipeline | All jobs green | GitHub Actions or local run succeeds |
| Crate count | 35+ total | `cargo metadata \| grep '"name"' \| wc -l` |
| Dependencies resolved | 100% | `cargo tree` shows no unresolved deps |
| Integration points | Structurally sound | Spot-check imports and trait alignment |

---

## 6. Potential Blockers & Mitigation

| Blocker | Likelihood | Mitigation |
|---------|-----------|-----------|
| RuVector crates have undeclared dependencies | Low | Re-audit curation audit; add missing deps |
| Cargo resolver conflicts | Low | Adjust `resolver = "2"` or pin versions |
| Path issues (crates not found) | Very low | Verify all 29 substrate crates have Cargo.toml |
| Incompatible Rust editions | Very low | All crates should be 2021 edition |
| Missing .gitignore files | Very low | Standard Rust .gitignore already in place |

---

## 7. Handoff to Phase 2

Once Phase 1 validates the workspace:
- **Phase 2 begins:** Implement `ruflo mcp serve` command with a hello-world tool
- **First integration test:** End-to-end MCP handshake with real Claude Code CLI
- **Scope:** 1 week
- **Deliverable:** Working MCP server that Claude Code can register and call

---

## 8. Key Principles for Phase 1

- **No implementation, only validation.** If we find a bug, we note it but don't fix it in Phase 1 — Phase 2 will address actual implementations.
- **Integration first.** Verify that Ruflo and RuVector crates can coexist in one workspace before Phase 2 builds on that foundation.
- **Clean state.** Phase 1 ends with a working tree ready for Phase 2's implementation work, with no loose ends.
- **Document discoveries.** Any surprises or integration issues found in Phase 1 become context for Phase 2 planning.

