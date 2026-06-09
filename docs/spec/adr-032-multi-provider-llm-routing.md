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

## Alternatives Considered

- **Hardcode Anthropic only**: current approach. Violates user's stated constraint. Rejected.
- **Port Claudish to Rust**: full TypeScript→Rust rewrite of the proxy. Overkill — the routing logic is 50 lines; the proxy server is not needed. Rejected.
- **Always use Prism**: satisfies preference but Prism is not always available (requires MCP server running). Use as one provider option, not the only one.
