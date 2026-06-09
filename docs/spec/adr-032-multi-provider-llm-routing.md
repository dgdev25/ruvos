# ADR-032: Multi-Provider LLM Routing (Claudish Patterns)

**Status:** Proposed  
**Date:** 2026-06-09  
**Source:** `/mnt/datadisk/repos/claudish` (TypeScript, MIT, 580+ models via OpenRouter)

## Context

Ruvos assumes a single LLM provider: Anthropic (via `ANTHROPIC_API_KEY`). `orchestrate_run` and `agent_spawn` hardcode Anthropic's API. For a large project build:

1. Some tasks are better suited to different models (Opus for architecture, Haiku for boilerplate, specialised code models for complex Rust)
2. Cost control requires routing cheap tasks to cheaper models
3. The user's requirement "I DO NOT want to use the Anthropic API key" means an alternative routing layer is essential

Claudish (`/mnt/datadisk/repos/claudish`) is a multi-provider LLM proxy (TypeScript, MIT) supporting 580+ models via OpenRouter with API translation, extended thinking, and context scaling. Its routing patterns are directly applicable to ruvos.

## Decision

Add provider abstraction to ruvos's LLM call layer:

1. Introduce a `LlmProvider` enum in `ruvos-mcp`: `Anthropic`, `OpenRouter`, `Ollama`, `Prism`
2. Read provider config from `~/.ruvos/llm.toml`:
   ```toml
   [default]
   provider = "openrouter"
   model = "anthropic/claude-sonnet-4-6"

   [archetypes.coder]
   provider = "openrouter"
   model = "anthropic/claude-opus-4-8"

   [archetypes.tester]
   provider = "openrouter"
   model = "anthropic/claude-haiku-4-5"
   ```
3. `agent_spawn` and `orchestrate_run` resolve the archetype to its configured provider + model before building the inference prompt
4. The Prism MCP tool (`mcp__prism__chat`) is exposed as a `Prism` provider option — satisfying the "use Prism, not bare API" preference in project memory

Claudish's TypeScript proxy is NOT ported to Rust; instead, ruvos calls the configured provider's REST API directly using `reqwest`. The routing logic (model selection per archetype) is the Claudish pattern to adopt, not its proxy architecture.

## Consequences

**Positive:**
- Closes the "no Anthropic API key" requirement: ruvos can route through OpenRouter or Prism
- Cost control: cheap tasks use cheap models; expensive tasks (architecture, security review) use Opus
- No vendor lock-in at the ruvos level

**Trade-offs:**
- `reqwest` dependency addition (likely already present; check workspace)
- `llm.toml` config file is new state to manage; must document format clearly
- Different providers have different rate limits and context windows — ruvos must handle provider-specific errors gracefully

## Prism Integration

Prism (`mcp__prism__*`) is the production deployment of what was originally PAL MCP Server — a multi-provider MCP bridge with 11+ tools (chat, planner, debug, codereview, thinkdeep, consensus, challenge, precommit, etc.) routing through OpenRouter.

Ruvos can integrate Prism at two levels:

### Level 1 — Provider alias (included in this ADR)

The `Prism` variant in `LlmProvider` routes LLM calls through `mcp__prism__chat` instead of a direct REST call. This satisfies the user's "use Prism/OpenRouter, not bare Anthropic API" constraint and is already captured in the `llm.toml` config above.

### Level 2 — Native tool mirroring (future ADR)

Prism's higher-order tools (thinkdeep, consensus, challenge) represent orchestration patterns that ruvos should eventually own natively:

| Prism tool | Equivalent ruvos concept | Target ADR |
|------------|--------------------------|------------|
| `consensus` | Multi-agent vote across archetypes | ADR-026 (agent_verify RARV) |
| `challenge` | Adversarial review of a claim | ADR-026 (agent_verify RARV) |
| `thinkdeep` | Extended reasoning pass before agent_exec | ADR-034 (spec validation) |
| `planner` | Structured goal decomposition | ADR-004 (GOAP planner) |
| `codereview` | Post-edit quality gate | ADR-025 (contract auto-check hook) |

Until those ADRs are implemented, ruvos calls Prism tools directly via MCP when the equivalent native capability is absent. This is not a long-term dependency — it is a bridge that shrinks as ruvos matures.

### Conversation threading

Prism manages conversation history across turns; ruvos's current `agent_spawn` creates stateless one-shot agents. The session system (ADR-001, `ruvos_session_*`) is the ruvos equivalent. Prism threading patterns should inform session design — specifically: thread IDs passed through `orchestrate_run` steps so multi-turn agent chains share context.

## Alternatives Considered

- **Hardcode Anthropic only**: current approach. Violates user's stated constraint. Rejected.
- **Port Claudish/PAL to Rust**: full rewrite of the proxy. Overkill — the routing logic is 50 lines; the proxy server is not needed. Rejected (PAL was already redeveloped into Prism).
- **Always use Prism**: satisfies preference but Prism is not always available (requires MCP server running). Use as one provider option, not the only one.
- **Replace Prism entirely with native ruvos tools**: correct long-term direction but not for this ADR. Native equivalents are tracked in ADR-026, ADR-034, and ADR-004.
