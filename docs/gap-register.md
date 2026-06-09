# Ruvos MCP Gap Register

Discovered by stress-testing all 52 MCP tools across two sessions (ForgeCMS Sprint 4–5, 2026-06-09).
Each gap is a concrete, reproducible finding — not speculation.

---

## Schema Gaps — RESOLVED

| # | Tool | Gap | Status |
|---|------|----|--------|
| 12 | `intel_pattern_store` | `trajectory` required field | ✅ Already in schema + validate() — was never missing |
| 13 | `intel_pattern_store` | `outcome` required field | ✅ Already in schema + validate() — was never missing |
| 7  | `hooks_post` | `success` boolean coercion | ✅ Fixed `0e7b810` — now accepts `true` and `"true"`/`"false"` |

---

## Agent Capability Gaps — RESOLVED

| # | Area | Gap | Status |
|---|------|-----|--------|
| 1 | `agent_spawn` | No filesystem write | ✅ Closed by `ruvos_agent_exec` (commit `2a9b0cd`) |
| 2 | `agent_spawn` | No shell/test execution | ✅ Closed by `ruvos_agent_exec` run_command op |
| 3 | `agent_spawn` | No git operations | ✅ Closed by `ruvos_agent_exec` git_op |
| 8 | Agent artifacts | No structured code output | ✅ Fixed `0e7b810` — `output_schema` param + `structured_output` response field |
| 11 | `orchestrate_run` | Agents echo task spec, do not invoke LLM | ✅ Fixed `ef9a2da` — calls Anthropic API when `ANTHROPIC_API_KEY` set |

---

## Infrastructure Gaps

| # | Tool/area | Gap | Status |
|---|-----------|-----|-------|
| 4 | `relay_send` | Stale presence — `delivered: false` | ✅ Fixed `acd6fcb` — `relay::announce_as()` writes named presence |
| 6 | Swarm | State in-memory only | ✅ Already resolved — `swarm::store()`/`current()` persist to `swarm.json` |
| 9 | `gov_cve_lookup` | Requires lockfile | By design — ensure `Cargo.lock` / `package-lock.json` exists |

---

## Field Name Corrections (already fixed — documented for reference)

All wrong field names below were corrected by adding typed JSON Schemas to the handlers in commit `ae5594c`.

| Tool | Wrong field used | Correct field |
|------|-----------------|---------------|
| `hooks_pre` | `type` | `kind` (enum: task/edit/command/session) |
| `hooks_post` | `type` | `kind`; also `success` must be boolean |
| `session_fork` | `session_id` | `source_session_id` |
| `relay_send` | `payload` | `body` |
| `relay_contract_resolve` | `contract_id` | `id` |
| `swarm_message` | `content` | `body` + routing key (`to`, `targets`, or `broadcast: true`) |
| `agent_spawn` | `role` | `archetype` (enum: coder/reviewer/tester) |
| `agent_message` | `content` | `message` |
| `compress_run` | `source` | `content` |
| `orchestrate_run` | `plan` | `template` (enum: feature/bugfix/refactor/security/sparc) + `task` |
| `gov_cve_lookup` | `query` | `project_path` |
| `gov_replay` | `filter` | `session_id` or `task_id` (one required) |
| `gov_swarm_recommendation` | `context` | `objective` / `task` / `goal` |

---

## Open Items + Improvement Backlog

Identified 2026-06-09 from stress-testing and large-project planning (ForgeCMS Sprints 7–9).
Priority: **P1** = blocks large project pipeline, **P2** = high value, **P3** = quality of life, **P4** = longer term.

### P1 — Blocks pipeline

| # | Area | Gap / Improvement | Notes |
|---|------|-------------------|-------|
| 10 | Cross-agent handoff | No automatic file passing between agents | Memory bridge is current workaround. Fix: `exchange_file` op writes to named swarm scratch slot; `read_slot` op consumes it. |
| 14 | `orchestrate_run` | No real multi-step pipeline driver | Templates generate a task spec only. Needed: spawn `coder` → await → spawn `reviewer` with output → await → spawn `tester`. Sequential pipeline, not just template expansion. |
| 15 | `agent_exec` | No checkpoint / resume on partial failure | A 12-op batch that fails on op 8 re-runs all 12 on retry. Need per-op journal so retry restarts from last successful op. |

### P2 — High value

| # | Area | Gap / Improvement | Notes |
|---|------|-------------------|-------|
| 5  | Plugin system | 0 plugins registered; `plugin_invoke` untestable | Build first real plugin: `forge-linter` (runs `cargo clippy` on a path). Proves dynamic dispatch end-to-end. |
| 16 | `memory_search` / `intel_pattern_search` | Keyword-only search, no semantic similarity | Embed a small HNSW index (`usearch` crate, MIT, 0-dep) or call an embedding endpoint. "Find patterns similar to this auth middleware" requires vector search. |
| 17 | `agent_exec` | All ops run sequentially, no parallelism | Add `parallel: true` flag or a `parallel_group` wrapper op so independent `write_file` ops fire concurrently. Will matter at Sprint 10+. |

### P3 — Quality of life

| # | Area | Gap / Improvement | Notes |
|---|------|-------------------|-------|
| 18 | `agent_exec` | No native patch/diff op | Patching large Rust files currently requires writing a Python script (workaround introduced Sprint 7). A `patch_file` op accepting unified diff or JSON patch would eliminate the double-brace Python pitfall. |
| 19 | `swarm_assign` | No task dependency graph | Tasks are independent. Add `depends_on: [task_id]` field; coordinator blocks assignment until deps reach `completed` state. Enables `[write tests → implement → run tests → review]` pipelines. |
| 20 | `gov_report` | Raw event dump, no sprint summary | Add `gov_sprint_summary(sprint_id)` returning: tasks completed, files changed, tests delta, agents used, wall-clock duration. Replaces manual progress tracking in docs/. |
| 21 | Contracts | No auto-check after mcp source edits | Wire a post-edit hook: when any file under `crates/ruvos-mcp/src/` is written via `agent_exec`, automatically run `ruvos contracts check`. Catches manifest drift before push. |

### P4 — Longer term

| # | Area | Gap / Improvement | Notes |
|---|------|-------------------|-------|
| 22 | `agent_exec` | No streaming progress for long ops | Large file writes and test runs >5s give no feedback. Emit progress events per op. |
| 23 | Session | No snapshot/resume across crashes | Persist swarm state + in-flight ops to `.rvf` snapshot so a crashed session can resume exactly. |
| 24 | Workspace | Single working-dir assumption | ForgeCMS has two roots: `forgecms/` (Rust) and `forgecms/admin/` (TypeScript). `agent_exec` handles this via per-op `cwd`, but other tools (hooks, gov) assume one root. Multi-workspace config needed. |

---

*Last updated: 2026-06-09. 53 tools. Gaps 1–4, 6–8, 11–13 resolved. Open: 10, 5, 14–24.*
