# ADR-034: AISP Symbolic Notation in hooks_pre — Prompt Precision Layer

**Status:** Implemented  
**Date:** 2026-06-09 (rewritten after reading aisp-open-core repo)  
**Implemented:** 2026-06-10 — phase 1 (hooks_pre, commit 339875d), phase 2 (agent_spawn, commit 830bab4)  
**Source:** `aisp-open-core` (AISP 5.1 Platinum Specification — MIT)

## Context

Ruvos agents receive free-form natural language prompts. Natural language has an ambiguity rate of 40–65%, producing inconsistent agent outputs and requiring clarification loops. AISP (AI Symbolic Protocol) is a formal notation standard that replaces ambiguous prose with mathematical symbols:

| Prose | AISP | Ambiguity |
|-------|------|-----------|
| "For all users, if admin then allow" | `∀u∈Users:admin(u)⇒allow(u)` | <2% |
| "Define x as 5" | `x≜5` | 0% |
| "There exists a valid solution" | `∃x:valid(x)` | 0% |

AISP 5.1 defines:
- **512 official symbols** across 8 categories (Transmuters, Topologics, Quantifiers, Contractors, Domains, Intents, Delimiters, Reserved)
- **Quality tiers** graded by semantic density (δ = ratio of symbolic to total content): Platinum (◊⁺⁺, δ≥0.75) → Gold (◊⁺, δ≥0.60) → Silver (◊, δ≥0.40) → Bronze (◊⁻, δ≥0.20) → Reject (⊘, δ<0.20)
- **Proof-carrying documents** with `⟦Ε⟧` evidence blocks

Native Rust crates are available:
- `aisp` v0.1.0 — document validation and tier computation
- `rosetta-aisp` v0.2.0 — bidirectional prose↔AISP conversion
- `rosetta-aisp-llm` v0.3.0 — LLM-fallback converter for low-confidence prose

## Decision

Add an AISP **prompt precision layer** to ruvos — wired into `hooks_pre` and `agent_spawn` — that converts incoming natural language task specs to AISP notation before agents receive them.

### 1. Conversion pipeline in `hooks_pre`

```
Incoming task prompt (prose)
  ↓
 rosetta-aisp: rule-based prose→AISP conversion
  ↓ (if confidence < threshold)
 rosetta-aisp-llm: LLM-enhanced conversion (via CliRouter, ADR-032)
  ↓
 aisp: validate document, compute tier (δ) and completeness (φ)
  ↓
 hooks_pre response: { original, aisp_spec, tier, delta, suggestions }
```

The converted AISP spec is attached to the `hooks_pre` response alongside the original. Downstream tools (`orchestrate_run`, `agent_spawn`) use the AISP spec when constructing agent prompts.

### 2. Quality gate (optional, config-driven)

`~/.ruvos/hooks.toml`:
```toml
[aisp]
enabled = true
min_tier = "silver"        # reject if δ < 0.40
warn_only = false           # if true: attach AISP but don’t block on low tier
auto_convert = true         # automatically convert prose; false = validate only if AISP already present
```

If `min_tier` is set and `warn_only = false`, `hooks_pre` returns `status: "blocked"` with the AISP diagnostic when the converted spec scores below threshold. The caller receives the AISP version and suggestions to improve density.

### 3. Rust crate integration

Add to `ruvos-mcp/Cargo.toml`:
```toml
aisp = "0.1"
rosetta-aisp = "0.2"
# rosetta-aisp-llm only if LLM fallback enabled in config
```

The `aisp` crate provides `Document::parse()` and `Document::tier()`. `rosetta-aisp` provides `convert(prose: &str) -> AispDoc`. No npm/external binary dependency.

### 4. AISP spec passed to agents

When `agent_spawn` or `orchestrate_run` constructs an agent prompt, if the `hooks_pre` response includes an `aisp_spec`, it is injected as a structured context block:

```
⟦Λ:Task⟧{
  ψ≜⟨implement, auth_middleware, rust⟩
  Pre: session_store_compliant(GDPR)
  Post: ∀r∈Routes:auth(r)⇒session_valid(r)
  Type: ⊤⇒Feature
}
```

Agents receiving AISP-formatted specs produce consistent, unambiguous outputs. The AI_GUIDE.md from aisp-open-core can be injected as system-prompt context to any agent so it understands the notation natively.

### 5. Tier reporting in `gov_health` / `gov_report`

The average AISP tier across recent agent calls becomes a governance metric: a declining δ trend indicates spec quality is degrading.

## Consequences

**Positive:**
- Ambiguity reduced from 40–65% → <2% for converted specs (AISP evidence: 97x pipeline success improvement)
- Pure Rust integration — no subprocess, no npm, no external binary
- Quality gating is optional and config-driven; existing workflows unchanged until `enabled = true`
- AISP tier is a measurable quality signal for governance reporting

**Trade-offs:**
- `rosetta-aisp` rule-based conversion is imperfect for complex prose; LLM fallback (`rosetta-aisp-llm`) adds a CliRouter call (ADR-032 required first)
- AISP notation is unfamiliar to humans reading logs; ruvos must always store the original prose alongside the AISP form
- Symbol density (δ) is a proxy for quality, not a guarantee — a spec full of symbols but missing intent still scores Platinum

## Dependency

ADR-032 (CliRouter) should be implemented first. `rosetta-aisp-llm` LLM fallback requires a working `CliRouter` instance. Rule-based conversion (`rosetta-aisp` only) can work without ADR-032.

## Alternatives Considered

- **Inline field-presence validator** (original ADR-034 design): checks for "intent", "success criteria" fields in plain prose. Brittle, language-specific, no formal grounding. Rejected.
- **LLM-only spec improvement**: ask a model to rewrite the prompt as a better spec. No verifiable quality signal, full LLM latency on every call. Rejected.
- **Shell out to `npx aisp-validator`**: works but adds Node.js dependency. Rust crates exist; use them. Rejected.
