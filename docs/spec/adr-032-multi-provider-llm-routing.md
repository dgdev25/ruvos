# ADR-032: Multi-Provider LLM Routing via CLI-First + OpenRouter Fallback

**Status:** Proposed  
**Date:** 2026-06-09 (revised after Prism/PAL clink analysis)

## Context

Ruvos currently hardcodes Anthropic's REST API via `ANTHROPIC_API_KEY`. This violates two hard constraints:

1. **`ANTHROPIC_API_KEY` must never be used** — the user exclusively uses subscription-based CLI tools and OpenRouter.
2. **CLI subscriptions are the primary compute** — `claude`, `gemini`, `codex` CLIs are already installed and authenticated via their respective subscription plans; they cost nothing beyond the subscription.

Prism (the production redevelopment of PAL MCP Server) solves this with its `clink` module: it detects which CLI executables are present at startup, routes inference calls through them via subprocess, and falls back to API-key providers (OpenRouter only) when CLIs are absent. Ruvos must adopt the same pattern natively in Rust.

## Routing Priority (hard-ordered, non-negotiable)

```
1. claude CLI   (subscription, no API key)    — `claude --print --output-format json`
2. gemini CLI   (subscription, no API key)    — `gemini -o json`
3. codex CLI    (subscription, no API key)    — `codex exec --json`
4. OpenRouter   (OPENROUTER_API_KEY only)     — REST to api.openrouter.ai

NEVER: ANTHROPIC_API_KEY or any direct Anthropic REST endpoint
NEVER: GEMINI_API_KEY, OPENAI_API_KEY, XAI_API_KEY as standalone keys
```

At startup, ruvos probes `$PATH` for each CLI executable. The first available CLI in priority order becomes the active provider. OpenRouter is used **only** if no CLI is found and `OPENROUTER_API_KEY` is set.

## Decision

### 1. `CliRouter` — new crate `ruvos-llm-router`

A lightweight Rust module that mirrors clink's architecture:

```rust
pub enum LlmProvider {
    ClaudeCli,    // `claude --print --output-format json [--model <m>]`
    GeminiCli,    // `gemini -o json [--model <m>]`
    CodexCli,     // `codex exec --json [--model <m>]`
    OpenRouter,   // REST: api.openrouter.ai/api/v1/chat/completions
}

pub struct CliRouter {
    provider: LlmProvider,
    model: Option<String>,    // passed to CLI via --model flag
    system_prompt: String,    // role-specific system prompt injected via --append-system-prompt
}
```

Detection at startup (in order):
```rust
fn detect_provider() -> LlmProvider {
    if which::which("claude").is_ok() { return LlmProvider::ClaudeCli; }
    if which::which("gemini").is_ok() { return LlmProvider::GeminiCli; }
    if which::which("codex").is_ok()  { return LlmProvider::CodexCli; }
    if std::env::var("OPENROUTER_API_KEY").is_ok() { return LlmProvider::OpenRouter; }
    panic!("No LLM provider available: install claude/gemini/codex CLI or set OPENROUTER_API_KEY");
}
```

### 2. Config: `~/.ruvos/llm.toml`

```toml
[routing]
# Priority order: first available wins. "auto" = use detect_provider().
priority = ["claude", "gemini", "codex", "openrouter"]

[claude]
model = "sonnet"                 # passed as --model sonnet
extra_args = ["--permission-mode", "acceptEdits"]

[gemini]
model = "gemini-2.5-pro"
extra_args = ["--yolo"]

[codex]
extra_args = ["--dangerously-bypass-approvals-and-sandbox", "--enable", "web_search_request"]

[openrouter]
# OPENROUTER_API_KEY read from environment, not stored here
default_model = "anthropic/claude-sonnet-4-6"

[archetypes]
# Per-archetype overrides: CLI/model selection for specific roles
coder.cli   = "claude"
coder.model = "sonnet"
tester.cli  = "gemini"
reviewer.cli = "codex"
```

### 3. Output parsing

Each CLI produces different JSON schema — parsers mirror Prism's `clink/parsers/`:

| CLI | Output format | Parse target |
|-----|--------------|---------------|
| `claude` | `{"type":"result","result":"..."}` | `.result` string |
| `gemini` | `{"candidates":[{"content":{"parts":[{"text":"..."}]}}]}` | `.candidates[0].content.parts[0].text` |
| `codex` | JSONL stream, last line `{"type":"message","content":"..."}` | last `.content` |
| OpenRouter | OpenAI-compatible: `.choices[0].message.content` | standard |

### 4. Integration points

- `agent_spawn`: resolves archetype → `CliRouter` → subprocess or REST call
- `orchestrate_run`: passes `model` hint from step config; router honours it if the CLI accepts `--model`
- `ruvos_agent_exec` (`cargo_check` etc.): unaffected — those call `cargo`, not an LLM

### 5. Prism as a meta-provider

When Prism MCP server is running, `mcp__prism__chat` and `mcp__prism__clink` are available as an alternative to direct CLI subprocess. This is the `Prism` provider option — it delegates routing to Prism's own clink registry rather than ruvos doing the subprocess itself. Useful when Prism is already running and clink config is managed there.

```toml
[routing]
priority = ["prism", "claude", "gemini", "codex", "openrouter"]
```

## Consequences

**Positive:**
- Zero API key management for primary inference — CLI subscriptions handle auth
- Prism's proven routing pattern adopted in Rust; no Python subprocess chain
- `OPENROUTER_API_KEY` is the only key ruvos ever reads; clearly documented
- Per-archetype CLI routing enables cost/capability optimisation without API spend

**Trade-offs:**
- CLI subprocess has higher latency than direct REST (∼100–300ms overhead per call for process spawn)
- CLI output formats vary; parsing is fragile against CLI version changes — parsers must be versioned
- `codex exec` runs in a sandboxed environment; some file-access patterns differ from direct API calls
- If all CLIs and OpenRouter are absent, ruvos hard-panics at startup — this is intentional (fail-fast over silent degradation)

## Alternatives Considered

- **Direct Anthropic REST**: violates the hard `ANTHROPIC_API_KEY` constraint. Rejected unconditionally.
- **Always use Prism MCP tool**: satisfies constraints but adds a process dependency. Use as a priority-0 option, not the only one.
- **Port Prism's full clink system to Rust**: correct direction. This ADR IS that port — `CliRouter` is the Rust-native clink.
