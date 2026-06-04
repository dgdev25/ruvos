# ADR-003: Rename the `workflow` tool domain to `orchestrate`

**Status:** Accepted (2026-06-03)
**Amends:** scope-ledger-v1.md §1 (the `workflow.run` tool)

## Context

The word "workflow" is heavily overloaded in the Claude Code ecosystem:

- Claude Code exposes a `/workflow` **slash command**.
- The harness provides a `Workflow` **multi-agent orchestration tool**.
- Plugins ship `workflow`-named **skills** (e.g. `ruflo-workflows:workflow`).
- rUvOS's own `workflow.run` MCP tool.

There is **no technical collision** — rUvOS's tool is namespaced by the MCP
server as `mcp__ruvos__workflow_run`, distinct from any slash command, harness
tool, or skill. But the overload creates **semantic ambiguity**: when a user says
"run a workflow," Claude must guess which "workflow" system is meant. Prefixing
the tool name with `ruvos-` was rejected as redundant (the `mcp__ruvos__` server
prefix already namespaces it).

## Decision

Rename the rUvOS tool **domain** `workflow` → **`orchestrate`**. The capability
(running an ordered, multi-agent pipeline from a template) is unchanged; only the
name changes to a distinct verb no other system claims, so natural-language
requests ("orchestrate a feature pipeline") route unambiguously to rUvOS.

Concrete renames:

| Before | After |
|--------|-------|
| domain `workflow`, tool `run` → `workflow.run` | domain `orchestrate`, tool `run` → `orchestrate.run` |
| arg `workflow_type` | arg `template` |
| response `workflow_id` | response `orchestration_id` |
| response `workflow_type` | response `template` |
| `tools/workflow.rs`, `WorkflowRunHandler` | `tools/orchestrate.rs`, `OrchestrateRunHandler` |

The template set is unchanged: `feature` (planner→coder→tester→reviewer),
`bugfix` (researcher→coder→tester), `refactor` (architect→coder→reviewer),
`security` (security→coder→tester).

Tool count is unchanged (24) — this is a rename, not an addition.

## Consequences

**Positive**
- Removes the semantic overload with the host `/workflow` command, the harness
  `Workflow` tool, and `workflow` skills. Plain-language requests are
  unambiguous.
- The arg/response field names (`template`, `orchestration_id`) read naturally on
  an `orchestrate` tool.

**Negative / trade-offs**
- Breaking change to the tool surface: any caller using `workflow.run` /
  `workflow_type` must switch to `orchestrate.run` / `template`. Acceptable —
  rUvOS is pre-1.0 (`v4.0.0-rc.1`) and the tool is new this development cycle.
- README and examples updated accordingly.

## Validation

- Covered by `crates/ruvos-mcp/src/tools/orchestrate.rs` tests.
- Exercised by `crates/ruvos-mcp/tests/integration_test.rs` through `orchestrate.run`.

## Alternatives considered

- **Prefix with `ruvos-`** (`ruvos-workflow.run`) — rejected: the `mcp__ruvos__`
  server prefix already namespaces every tool, so this just doubles to
  `mcp__ruvos__ruvos-workflow_run`.
- **Leave `workflow` as-is** — valid (namespacing prevents real collisions), but
  the user opted to remove the semantic ambiguity at the source.
