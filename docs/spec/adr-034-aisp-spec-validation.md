# ADR-034: AISP Spec Validation in hooks_pre

**Status:** Proposed  
**Date:** 2026-06-09  
**Source:** `aisp-open-core` (formal spec validator, 512-symbol grammar, quality tier grading)

## Context

Ruvos agents receive free-form task prompts. There is currently no validation step that checks whether a prompt is a well-formed, actionable spec before it is executed. Vague, ambiguous, or underspecified prompts reach `agent_exec` and `orchestrate_run` unchanged — producing low-quality or incorrect output.

`aisp-open-core` is a formal spec validator with:
- A 512-symbol grammar (intent, constraints, success criteria, context, scope)
- Quality tier grading: `S` (complete) → `A` → `B` → `C` → `F` (unparseable)
- Structured output (`--json`) with missing-field diagnostics and improvement suggestions

Wiring it into `hooks_pre` would gate every task before an agent fires — the same hook already used for routing and context injection.

## Decision

Add spec validation as an optional `hooks_pre` stage:

1. **`HooksPreHandler` extended with a `spec_validate` stage** — runs after routing, before returning. If enabled in `~/.ruvos/hooks.toml`, it calls the AISP validator on the incoming `task` payload.

2. **Config gate in `hooks.toml`**:
   ```toml
   [spec_validation]
   enabled = true
   min_tier = "B"          # reject tiers C and F
   warn_only = false        # if true, log warning but don't block
   ```

3. **Validation result in `hooks_pre` response**:
   ```json
   {
     "routing": "...",
     "spec": {
       "tier": "A",
       "score": 87,
       "missing": ["success_criteria"],
       "suggestions": ["Add a measurable success condition"]
     }
   }
   ```
   If tier < `min_tier` and `warn_only = false`, `hooks_pre` returns `status: "blocked"` with the spec diagnostics so the caller can improve the prompt before retrying.

4. **Implementation**: AISP is not rewritten in Rust for this ADR. `hooks_pre` shells out to the `aisp` binary via `PluginExecutor` (same pattern as `run_command`). A future ADR can embed the grammar as a Rust crate.

5. **`ruvos_orchestrate_run` integration**: when `hooks_pre` blocks with `status: "blocked"` and a `spec` payload, the orchestrator surfaces the diagnostics to the caller rather than proceeding.

## Consequences

**Positive:**
- Catches underspecified prompts before they waste LLM tokens on an agent run
- Quality tier grading gives callers actionable feedback, not just a rejection
- Config gate makes it opt-in — existing workflows unchanged until `enabled = true`
- `warn_only` mode lets teams observe quality without breaking anything

**Trade-offs:**
- Shell-out adds latency (~50–150ms) to every `hooks_pre` call when enabled
- Requires `aisp` binary on `$PATH`; ruvos should degrade gracefully if it is absent (log a warning, skip validation)
- Grammar coverage: AISP's 512-symbol grammar is designed for structured software specs, not all prompt types. Free-form creative prompts may tier as `C` even when they are perfectly actionable.

## Alternatives Considered

- **Inline Rust grammar parser**: highest performance, no dependency, but 512-symbol grammar is non-trivial to maintain in Rust. Deferred to a future ADR.
- **LLM-based quality check**: ask a cheap model to grade the spec. More flexible grammar but adds a full LLM round-trip latency. Rejected for `hooks_pre` (too slow for a gate).
- **Client-side only**: validate at the CLI layer before calling ruvos. Doesn't protect the MCP API surface. Rejected.
