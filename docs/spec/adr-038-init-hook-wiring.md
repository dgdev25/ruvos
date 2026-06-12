# ADR-038: Deterministic Hook Wiring via `ruvos init --hooks`

**Status:** Implemented  
**Date:** 2026-06-12

## Context

The learning loop (SONA learning, auto-swarm creation, safety checks, the
durable event log) only fires when the model voluntarily calls the
`ruvos_hooks_pre` / `ruvos_hooks_post` MCP tools. The managed CLAUDE.md block
(ADR-036) instructs it to, but instruction-following is probabilistic: under
context pressure or in subagents that never saw the block, hooks are silently
skipped and the learning layer goes blind for the whole session.

Claude Code has a mechanical alternative: `.claude/settings.json` hook
bindings. The harness invokes a configured shell command on PreToolUse /
PostToolUse / SessionStart / Stop, passing the hook event as JSON on stdin; a
zero exit code means continue.

## Decision

Wire rUvOS hooks into the harness deterministically, opt-in:

1. **`ruvos hook <kind> --phase <pre|post>` subcommand**
   (`crates/ruvos-cli/src/commands/hook.rs`). Reads the harness's hook event
   JSON from stdin and dispatches it in-process through the same
   `HooksPreHandler` / `HooksPostHandler` the MCP tools use (ruvos-cli already
   depends on ruvos-mcp), so SONA learning / auto-swarm / event log fire
   whether or not the model remembers to call the tools itself.
2. **`ruvos init --hooks` flag** — merges hook bindings into
   `.claude/settings.json` via
   `crates/ruvos-cli/src/commands/init_hooks.rs::write_hook_bindings`:
   - PreToolUse(`Edit|Write`) → `ruvos hook edit --phase pre`
   - PostToolUse(`Edit|Write`) → `ruvos hook edit --phase post`
   - PreToolUse(`Bash`) → `ruvos hook command --phase pre`
   - PostToolUse(`Bash`) → `ruvos hook command --phase post`
   - SessionStart → `ruvos hook session --phase pre`
   - Stop → `ruvos hook session --phase post`
3. **Merge, don't clobber.** The writer parses the existing settings.json,
   preserves all user-defined entries and unrelated keys, and is idempotent —
   re-running `ruvos init --hooks` adds no duplicates. The file is created
   (including `.claude/`) when absent. Invalid JSON is an error, never
   overwritten.

## Consequences

- Pre/post hooks fire deterministically on every edit, command, and session
  boundary — the learning loop no longer depends on model compliance.
- `ruvos hook` always exits 0: dispatch errors are printed to stderr and
  swallowed, so a learning-layer failure can never block the user's edit or
  command.
- The wiring is opt-in (`--hooks`); plain `ruvos init` behavior is unchanged.
- Task-kind hooks (`ruvos_hooks_pre kind=task`) still rely on the model — the
  harness has no "task start" event — so the managed CLAUDE.md block keeps
  that instruction, and now notes that edit/command/session hooks fire
  automatically when bindings are installed.
- Bindings reference the `ruvos` binary by name; it must be on PATH for the
  harness (true for the standard global install, ADR-033).
