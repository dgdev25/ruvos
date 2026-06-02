# Phase 7: Cutover (Final Release) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use subagent-driven-development to implement this plan task-by-task.

**Goal:** Complete the Ruflo v4 rewrite cutover: publish binary, archive legacy TypeScript code, create release tag.

**Architecture:** Phase 7 is the release phase:
1. Mark code as v4.0.0-rc.1 ready
2. Archive legacy TypeScript Ruflo (v2/v3) code to `legacy/` directory
3. Create git tag for v4.0.0-rc.1
4. Document release in CLAUDE.md

**Scope:** This is a packaging/release phase, not feature development.

---

## Task Breakdown

### Task 1: Prepare for Release

**Files:**
- Verify all tests pass
- Verify no warnings or errors

**Steps:**
- [ ] Run `cargo test --all-features` — all tests must pass
- [ ] Run `cargo clippy --all-targets --all-features -- -D warnings` — zero warnings
- [ ] Run `cargo build --release` — release build must succeed
- [ ] Update `CLAUDE.md` with final status

---

### Task 2: Create Release Tag

**Files:**
- Git tag: v4.0.0-rc.1

**Steps:**
- [ ] Create annotated git tag: `git tag -a v4.0.0-rc.1 -m "Ruflo v4.0.0-rc.1: Complete Rust rewrite with plugin host, hooks, and multi-CLI support"`
- [ ] Verify tag created: `git tag -l v4.0.0-rc.1`

---

### Task 3: Final Documentation Update

**Files:**
- Modify: `CLAUDE.md`

**Steps:**
- [ ] Append Phase 7 completion section
- [ ] Mark entire rewrite as complete
- [ ] Document release tag

---

## Success Criteria

1. All tests pass (58+ tests)
2. Release build succeeds
3. Zero clippy warnings
4. v4.0.0-rc.1 tag created
5. CLAUDE.md documents completion
6. All 7 phases marked complete

---
