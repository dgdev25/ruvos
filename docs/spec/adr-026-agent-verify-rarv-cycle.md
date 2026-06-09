# ADR-026: agent_verify Tool — RARV Verification Cycle (from Loki-Mode)

**Status:** Proposed  
**Date:** 2026-06-09  
**Source:** `/mnt/datadisk/repos/Loki-Mode` (TypeScript, 684 lines, MIT)

## Context

Ruvos agents produce artifacts (code, analysis, plans) but there is no built-in verification step. An agent can claim "tests pass" without running them, or a coder agent can produce syntactically valid but logically wrong code with no adversarial check. The Loki-Mode repository implements a proven RARV cycle (Reason → Act → Reflect → Verify) with anti-sycophancy detection across 41 agent types.

For a large project build, unverified agent output is the primary source of regressions. Loki-Mode's pattern is the right counter-measure.

## Decision

Add `ruvos_agent_verify` tool that accepts:
- `artifact`: the content to verify (code, plan, or analysis text)
- `artifact_type`: `code` | `plan` | `analysis`
- `verifier_archetype`: archetype to use for the Reflect step (default: `reviewer`)
- `criteria`: optional list of specific checks to enforce

Internal execution (rewritten in Rust from Loki-Mode TypeScript):
1. **Reason**: extract claims made in the artifact ("tests pass", "no XSS", "handles edge case X")
2. **Act**: for each claim, generate a targeted refutation prompt
3. **Reflect**: spawn a `reviewer` archetype agent with the refutation prompt; collect verdict
4. **Verify**: if ≥2 of N refuters find the claim false, mark claim `unverified` and include in response

Returns: `{verified: bool, unverified_claims: [...], confidence: 0.0-1.0, summary: string}`.

Anti-sycophancy: refuter prompts explicitly instruct the agent to default to `refuted: true` if uncertain, preventing "sounds reasonable" hallucination.

## Consequences

**Positive:**
- Every sprint's coder output can be passed through `agent_verify` before commit
- Anti-sycophancy detection catches the most common LLM failure mode: confident but wrong
- Reuses existing `agent_spawn` infrastructure — no new execution engine needed

**Trade-offs:**
- Adds latency: each verify call spawns 1-3 additional agents
- Requires `ANTHROPIC_API_KEY` (or configured LLM provider) to function
- False positives possible: a correct but unusual pattern may be flagged

## Rewrite Plan

Loki-Mode RARV core is ~200 lines of TypeScript logic. Rust rewrite target: `crates/ruvos-mcp/src/tools/verify.rs`, implementing `AgentVerifyHandler` following existing handler patterns.
