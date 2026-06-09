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
| 10 | Agent pipeline | No cross-agent file passing | Mitigated: agent_exec + memory bridge. Full automatic handoff still future work. |
| 11 | `orchestrate_run` | Agents echo task spec, do not invoke LLM | ✅ Fixed `ef9a2da` — calls Anthropic API when `ANTHROPIC_API_KEY` set |

---

## Infrastructure Gaps

| # | Tool/area | Gap | Status |
|---|-----------|-----|-------|
| 4 | `relay_send` | Stale presence — `delivered: false` | ✅ Fixed `acd6fcb` — `relay::announce_as()` writes named presence |
| 5 | Plugin system | 0 plugins registered | Open — needs first real plugin |
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

## Open Items (pre-Sprint 9)

| # | Area | Gap |
|---|------|-----|
| 10 | Cross-agent handoff | Full automatic file passing between pipeline agents — memory bridge is a workaround |
| 5 | Plugin system | No plugins registered; `plugin_invoke` untestable until a real plugin is written |

---

*Last updated: 2026-06-09. 53 tools. Gaps 1–4, 6–8, 11–13 resolved. 2 open items remain.*
