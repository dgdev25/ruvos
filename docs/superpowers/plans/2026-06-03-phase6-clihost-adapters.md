# Phase 6: CliHost Adapters Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Implement CliHost trait adapters for Claude Code and Codex CLI, normalizing event streams from both to unified Ruflo execution model.

**Architecture:** Phase 6 implements the `CliHost` trait + two adapters (Claude, Codex) that normalize CLI events (tool calls, responses, errors) into Ruflo's internal format. Single Ruflo binary runs on multiple CLI platforms via adapter abstraction.

**Tech Stack:** Rust 1.77+, tokio, serde_json, tracing.

---

## Task Breakdown

### Task 1: Define CliHost Trait and Event Types

**Files:**
- Modify: `crates/ruflo-host/src/lib.rs` — Define `CliHost` trait and event types

**Steps:**

- [ ] Define `CliHost` trait with methods:
  - `async fn send_tool_call(&self, tool_call: ToolCall) -> Result<()>`
  - `async fn receive_response(&self) -> Result<ToolResponse>`
  - `async fn report_error(&self, error: CliError) -> Result<()>`

- [ ] Define `ToolCall`, `ToolResponse`, `CliError` types

- [ ] Commit: "feat: define CliHost trait abstraction"

---

### Task 2: Implement Claude Code Adapter

**Files:**
- Create: `crates/ruflo-host/src/adapters/claude.rs` — Claude Code CLI adapter

**Steps:**

- [ ] Implement `ClaudeCliHost` struct implementing `CliHost`
- [ ] Handle Claude-specific event format
- [ ] Normalize to `ToolCall` / `ToolResponse` types
- [ ] Commit: "feat: implement Claude Code CLI adapter"

---

### Task 3: Implement Codex CLI Adapter

**Files:**
- Create: `crates/ruflo-host/src/adapters/codex.rs` — Codex CLI adapter

**Steps:**

- [ ] Implement `CodexCliHost` struct implementing `CliHost`
- [ ] Handle Codex-specific event format
- [ ] Normalize to `ToolCall` / `ToolResponse` types
- [ ] Commit: "feat: implement Codex CLI adapter"

---

### Task 4: Integration Tests + Documentation

- [ ] Write tests for both adapters
- [ ] Verify event round-trip through Ruflo
- [ ] Update CLAUDE.md with Phase 6 completion
- [ ] Commit: "test: add CliHost adapter integration tests" + "docs: Phase 6 completion documented"

---

## Success Criteria

1. `CliHost` trait defined and usable by multiple implementations
2. Claude Code adapter normalizes Claude events correctly
3. Codex CLI adapter normalizes Codex events correctly
4. Tests verify event round-trip
5. All workspace tests pass
6. Documentation updated

---
