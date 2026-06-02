# Phase 5: Memory, Session, and Agent Tools Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the 10 real tool handlers (4 memory, 3 session, 3 agent) that integrate with RuVector substrate for vector search, session persistence, and agent spawning.

**Architecture:** Phase 5 implements the core orchestration logic: vector-backed semantic memory search with MMR + recency weighting, `.rvf` container persistence for sessions, and agent archetype spawning with trait composition.

**Tech Stack:** Rust 1.77+, tokio, ruvector-core (HNSW), sona (reranking), rvf-types (containers), UUID, serde_json.

**Total new LOC budget:** ~2,500 (across memory, session, agent implementations)

---

## Task Breakdown (Simplified for Speed)

### Task 1: Memory Tools (memory.search, memory.store, memory.retrieve, memory.list)

**Files:**
- Modify: `crates/ruflo-mcp/src/tools/memory.rs` — Implement 4 real handlers using ruvector-core

**Steps:**

- [ ] **Implement MemorySearchHandler:**
  - Use `ruvector_core::HNSW` for semantic search
  - Apply MMR diversity + recency weighting
  - Return top-K results with scores

- [ ] **Implement MemoryStoreHandler:**
  - Accept entry with embedding + tags
  - Insert into HNSW index
  - Persist to redb or SQLite

- [ ] **Implement MemoryRetrieveHandler:**
  - Get single entry by key
  - Return full entry with metadata

- [ ] **Implement MemoryListHandler:**
  - List entries in namespace with filters
  - Support pagination

- [ ] **Commit:** "feat: implement memory tools with HNSW search"

---

### Task 2: Session Tools (session.create, session.resume, session.fork)

**Files:**
- Modify: `crates/ruflo-mcp/src/tools/session.rs` — Implement 3 real handlers using rvf

**Steps:**

- [ ] **Implement SessionCreateHandler:**
  - Create new `.rvf` container
  - Initialize session state
  - Return session ID

- [ ] **Implement SessionResumeHandler:**
  - Load existing `.rvf` container
  - Restore full context + memory
  - Return session state

- [ ] **Implement SessionForkHandler:**
  - COW-branch existing session
  - Create new session from snapshot
  - Return forked session ID

- [ ] **Commit:** "feat: implement session tools with .rvf containers"

---

### Task 3: Agent Tools (agent.spawn, agent.status, agent.message)

**Files:**
- Modify: `crates/ruflo-mcp/src/tools/agent.rs` — Implement 3 real handlers

**Steps:**

- [ ] **Implement AgentSpawnHandler:**
  - Parse archetype + traits from params
  - Spawn agent with model + budget
  - Return agent ID + status

- [ ] **Implement AgentStatusHandler:**
  - List running agents
  - Return state + metrics per agent

- [ ] **Implement AgentMessageHandler:**
  - Send message to named agent
  - Queue message in agent's inbox
  - Return confirmation

- [ ] **Commit:** "feat: implement agent tools with archetype dispatch"

---

### Task 4: Full Integration Tests + Workspace Validation

- [ ] Write end-to-end integration tests for all 10 tools
- [ ] Run full test suite: `cargo test --all-features`
- [ ] Verify all RuVector substrate crates load correctly
- [ ] Commit: "test: add Phase 5 integration tests"

---

### Task 5: Update Documentation

- [ ] Append Phase 5 completion section to CLAUDE.md
- [ ] Commit: "docs: Phase 5 completion documented"

---

## Success Criteria

**Phase 5 is complete when:**
1. All 10 tools are implemented and working
2. Memory search integrates with HNSW
3. Sessions persist to `.rvf` containers
4. Agents spawn with archetypes
5. All tests pass
6. RuVector substrate crates validate

---
