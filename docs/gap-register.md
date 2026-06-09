# Ruvos MCP Gap Register

Discovered by stress-testing all 52 MCP tools across two sessions (ForgeCMS Sprint 4–5, 2026-06-09).
Each gap is a concrete, reproducible finding — not speculation.

---

## Schema Gaps (highest priority — break MCP client type safety)

| # | Tool | Missing field | Symptom |
|---|------|--------------|---------|
| 12 | `intel_pattern_store` | `trajectory` (array of strings) | Call fails: "missing 'trajectory' field" |
| 13 | `intel_pattern_store` | `outcome` (string) | Call fails: "missing 'outcome' field" |
| 7  | `hooks_post` | `success` typed as `boolean` in schema | MCP client serialises as string `"true"` — server rejects unless restarted after schema commit |

**Fix:** Add `trajectory` and `outcome` to the `intel_pattern_store` JSON Schema `required` array in `intel.rs`. Restart server after any schema change to apply typed coercion.

---

## Agent Capability Gaps (medium priority — limits what agents can do)

| # | Area | Gap | Impact |
|---|------|-----|--------|
| 1 | `agent_spawn` | No filesystem write | Agents produce markdown artifacts only; cannot write `.ts`, `.rs`, or any source file |
| 2 | `agent_spawn` | No shell/test execution | Cannot run `vitest`, `cargo test`, `npm build`, etc. |
| 3 | `agent_spawn` | No git operations | Cannot commit, diff, read log, or read git blame |
| 8 | Agent artifacts | No structured code output | Artifact format is free markdown; no parseable code blocks, no typed JSON output schema |
| 10 | Agent pipeline | No cross-agent file passing | Agent A cannot hand a file artifact to Agent B automatically; requires the orchestrator to read and re-pass content |
| 11 | `orchestrate_run` | ~~Agents echo task spec, do not invoke LLM~~ **Fixed (ef9a2da)** — `src/llm.rs` added; `run_task` calls Anthropic API when `ANTHROPIC_API_KEY` is set, falls back to template otherwise | Set `ANTHROPIC_API_KEY` |

---

## Infrastructure Gaps (lower priority — workarounds exist)

| # | Tool/area | Gap | Workaround |
|---|-----------|-----|-----------|
| 4 | `relay_send` | Stale presence — `delivered: false` unless target has active recent `relay_announce` | Poll with heartbeat; accept delivered:false as normal |
| 5 | Plugin system | 0 plugins registered — `plugin_invoke` untestable | N/A until first plugin is written |
| 6 | Swarm | ~~State is in-memory only — lost on process restart~~ **Already resolved** — `swarm::store()`/`current()` write/read JSON files in `data_root/swarm.json` | — |
| 9 | `gov_cve_lookup` | Requires a lockfile (`package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`, `Cargo.lock`) — exits early for projects without one | Ensure lockfile exists before calling; note that Rust projects need `Cargo.lock` committed |

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

## Proposed Prioritisation

1. **Fix schema gaps 12 + 13** (`intel_pattern_store` missing required fields) — one-line Rust change each, immediate call-site fix
2. **Fix gap 7** (`hooks_post` boolean coercion) — server restart resolves it; schema already committed
3. **Gap 11** (`orchestrate_run` LLM inference) — highest value unlock; turns orchestration into a real planning tool
4. **Gap 1–3** (agent filesystem/shell/git) — enables autonomous TDD cycles; requires sandboxed execution environment
5. **Gap 6** (swarm persistence) — necessary for long-running multi-session workflows

---

*Last updated: 2026-06-09. 52 tools exercised; 13 gaps identified. Gap 6 was inaccurate (already resolved). Gap 11 fixed in ef9a2da.*
