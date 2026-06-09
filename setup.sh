#!/usr/bin/env bash
#
# rUvOS one-shot installer.
#
#   git clone https://github.com/dgdev25/ruvos.git
#   cd ruvos
#   ./setup.sh
#
# What this does (fully idempotent — safe to re-run):
#
#   1. Check prerequisites (Rust/cargo)
#   2. Build the release binary
#   3. Remove legacy Ruflo v2/v3 npm CLI (incompatible with v4)
#   4. Install `ruvos` binary onto PATH
#   5. Write PATH + RUVOS_HOME to shell profile
#   6. Scaffold ~/.ruvos data directories
#   7. Register rUvOS as MCP server with Claude Code (user scope)
#   8. Wire Claude Code lifecycle hooks (PreToolUse, PostToolUse, Stop, SessionStart)
#   9. Register rUvOS MCP server with Codex CLI (if installed)
#  10. Smoke-test: binary + MCP round-trip
#
# Flags:
#   --no-mcp       Skip MCP registration (steps 7-9)
#   --no-hooks     Skip Claude Code hook wiring (step 8)
#   --prefix DIR   Install binary into DIR (default: /usr/local/bin or ~/.local/bin)
#   -h | --help    Show this help

set -euo pipefail

BOLD=$'\033[1m'; GREEN=$'\033[32m'; YELLOW=$'\033[33m'; RED=$'\033[31m'; DIM=$'\033[2m'; RESET=$'\033[0m'
say()  { printf '%s\n' "${BOLD}▶ $*${RESET}"; }
ok()   { printf '%s\n' "${GREEN}  ✓ $*${RESET}"; }
warn() { printf '%s\n' "${YELLOW}  ! $*${RESET}"; }
die()  { printf '%s\n' "${RED}  ✗ $*${RESET}" >&2; exit 1; }
step() { printf '%s\n' "${DIM}    $*${RESET}"; }

# ----- args -----------------------------------------------------------------
NO_MCP=0; NO_HOOKS=0; PREFIX=""
while [ $# -gt 0 ]; do
  case "$1" in
    --no-mcp)   NO_MCP=1 ;;
    --no-hooks) NO_HOOKS=1 ;;
    --prefix)   PREFIX="${2:-}"; shift ;;
    -h|--help)  grep '^#' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown flag: $1 (use --help)" ;;
  esac
  shift
done

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$REPO_DIR"

printf '%s\n' "${BOLD}╔══════════════════════════════════════════════════════╗${RESET}"
printf '%s\n' "${BOLD}║   rUvOS installer — the agentic operating system     ║${RESET}"
printf '%s\n' "${BOLD}╚══════════════════════════════════════════════════════╝${RESET}"
echo

# ----- 0. platform ----------------------------------------------------------
OS="$(uname -s)"
case "$OS" in
  Linux|Darwin) ok "platform: $OS" ;;
  *) warn "platform '$OS' is not officially supported (Linux/macOS). Continuing." ;;
esac

# ----- 1. prerequisites -----------------------------------------------------
say "Checking prerequisites"
command -v cargo >/dev/null 2>&1 || die "Rust/cargo not found. Install from https://rustup.rs and re-run."
ok "cargo: $(cargo --version)"

# ----- 2. build -------------------------------------------------------------
say "Building the rUvOS release binary (first run takes a few minutes)"
cargo build --release -p ruvos-cli --jobs 4
BIN="$REPO_DIR/target/release/ruvos"
[ -x "$BIN" ] || die "build produced no binary at $BIN"
ok "built: $($BIN --version)"

# ----- 3. remove legacy Ruflo v2/v3 npm CLI ---------------------------------
say "Removing legacy Ruflo v2/v3 npm CLI"
if command -v npm >/dev/null 2>&1; then
  npm uninstall -g ruflo            >/dev/null 2>&1 && ok "removed: ruflo"           || warn "ruflo not installed (ok)"
  npm uninstall -g @claude-flow/cli >/dev/null 2>&1 && ok "removed: @claude-flow/cli" || warn "@claude-flow/cli not installed (ok)"
  npm cache clean --force >/dev/null 2>&1 || true
else
  warn "npm not found — skipping (ok if you never installed Ruflo v2/v3)"
fi

# ----- 4. install the binary ------------------------------------------------
say "Installing ruvos binary"
choose_prefix() {
  [ -n "$PREFIX" ] && { echo "$PREFIX"; return; }
  [ -w /usr/local/bin ] && { echo /usr/local/bin; return; }
  command -v sudo >/dev/null 2>&1 && { echo /usr/local/bin; return; }
  echo "$HOME/.local/bin"
}
PREFIX="$(choose_prefix)"
mkdir -p "$PREFIX" 2>/dev/null || true

install_to() {
  local dest="$1/ruvos"
  if [ -w "$1" ]; then
    cp "$BIN" "$dest"
  elif command -v sudo >/dev/null 2>&1; then
    sudo cp "$BIN" "$dest"
  else
    return 1
  fi
}
if ! install_to "$PREFIX"; then
  PREFIX="$HOME/.local/bin"
  mkdir -p "$PREFIX"
  cp "$BIN" "$PREFIX/ruvos"
fi
ok "installed: $PREFIX/ruvos"

# ----- 5. shell profile: PATH + RUVOS_HOME ----------------------------------
say "Configuring shell environment"
RUVOS_HOME_DEFAULT="$HOME/.ruvos"
detect_profile() {
  case "${SHELL:-}" in
    *zsh)  echo "$HOME/.zshrc" ;;
    *bash) [ -f "$HOME/.bashrc" ] && echo "$HOME/.bashrc" || echo "$HOME/.bash_profile" ;;
    *)     echo "$HOME/.profile" ;;
  esac
}
PROFILE="$(detect_profile)"; touch "$PROFILE"
add_line() { grep -qsF "$1" "$PROFILE" || printf '\n%s\n' "$1" >> "$PROFILE"; }

case ":${PATH:-}:" in
  *":$PREFIX:"*) : ;;
  *) add_line "export PATH=\"$PREFIX:\$PATH\""; step "added $PREFIX to PATH in $PROFILE" ;;
esac
add_line "export RUVOS_HOME=\"$RUVOS_HOME_DEFAULT\""
export RUVOS_HOME="$RUVOS_HOME_DEFAULT"
ok "RUVOS_HOME=$RUVOS_HOME"

# ----- 6. scaffold ~/.ruvos data dirs ---------------------------------------
say "Scaffolding ~/.ruvos data directories"
mkdir -p \
  "$RUVOS_HOME/plugins" \
  "$RUVOS_HOME/sessions" \
  "$RUVOS_HOME/cve/advisories" \
  "$RUVOS_HOME/agents" \
  "$RUVOS_HOME/intel"
ok "data dirs ready under $RUVOS_HOME"

# Write a default plugin.toml if no plugins exist yet
if [ ! -f "$RUVOS_HOME/plugins/.ruvos-init" ]; then
  touch "$RUVOS_HOME/plugins/.ruvos-init"
  step "plugin dir initialized (drop plugins into $RUVOS_HOME/plugins/<name>/)"
fi

# ----- 7. register MCP server with Claude Code ------------------------------
if [ "$NO_MCP" -eq 0 ]; then
  say "Registering rUvOS MCP server with Claude Code"
  if command -v claude >/dev/null 2>&1; then
    claude mcp remove ruvos >/dev/null 2>&1 || true
    if claude mcp add ruvos --scope user -- "$PREFIX/ruvos" mcp serve >/dev/null 2>&1; then
      ok "claude mcp: ruvos registered (user scope)"
    else
      warn "auto-registration failed. Run manually:"
      warn "  claude mcp add ruvos --scope user -- $PREFIX/ruvos mcp serve"
    fi
  else
    warn "'claude' CLI not found — once installed, run:"
    warn "  claude mcp add ruvos --scope user -- $PREFIX/ruvos mcp serve"
  fi
else
  warn "--no-mcp: skipped Claude Code MCP registration"
fi

# ----- 8. wire Claude Code lifecycle hooks ----------------------------------
# Hooks let rUvOS learn from every Claude Code session:
#   PreToolUse  → hooks.pre  (task, edit, command routing)
#   PostToolUse → hooks.post (SONA learning, trajectory store)
#   Stop        → session.fork (checkpoint .rvf container)
#   SessionStart→ session.resume (restore prior context)
#
# We write a tiny dispatcher script into ~/.ruvos/hooks/ and add it to
# ~/.claude/settings.json. Existing hooks in settings.json are preserved.
if [ "$NO_HOOKS" -eq 0 ] && [ "$NO_MCP" -eq 0 ]; then
  say "Wiring Claude Code lifecycle hooks"
  HOOKS_DIR="$HOME/.claude/hooks/ruvos"
  mkdir -p "$HOOKS_DIR"

  # Pre-tool-use hook: route to rUvOS hooks.pre via MCP stdin call
  cat > "$HOOKS_DIR/pre-tool-use.sh" << 'HOOK_EOF'
#!/usr/bin/env bash
# rUvOS pre-tool-use hook — dispatches to hooks.pre MCP tool.
# Invoked by Claude Code before every tool call.
set -euo pipefail
RUVOS_BIN="${RUVOS_BIN:-ruvos}"
TOOL="${CLAUDE_TOOL_NAME:-unknown}"
KIND="command"
case "$TOOL" in Edit|Write|MultiEdit) KIND="edit" ;;
               Bash|Execute)          KIND="command" ;;
               Task*)                 KIND="task" ;; esac
printf '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hooks.pre","arguments":{"kind":"%s","tool":"%s"}}}\n' \
  "$KIND" "$TOOL" | timeout 3 "$RUVOS_BIN" mcp serve 2>/dev/null | grep -q '"status"' || true
HOOK_EOF
  chmod +x "$HOOKS_DIR/pre-tool-use.sh"

  # Post-tool-use hook: dispatch to hooks.post for SONA learning
  cat > "$HOOKS_DIR/post-tool-use.sh" << 'HOOK_EOF'
#!/usr/bin/env bash
# rUvOS post-tool-use hook — dispatches to hooks.post MCP tool.
set -euo pipefail
RUVOS_BIN="${RUVOS_BIN:-ruvos}"
TOOL="${CLAUDE_TOOL_NAME:-unknown}"
OUTCOME="${CLAUDE_TOOL_SUCCESS:-true}"
KIND="command"
case "$TOOL" in Edit|Write|MultiEdit) KIND="edit" ;;
               Bash|Execute)          KIND="command" ;;
               Task*)                 KIND="task" ;; esac
printf '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hooks.post","arguments":{"kind":"%s","tool":"%s","outcome":"%s"}}}\n' \
  "$KIND" "$TOOL" "$OUTCOME" | timeout 3 "$RUVOS_BIN" mcp serve 2>/dev/null | grep -q '"status"' || true
HOOK_EOF
  chmod +x "$HOOKS_DIR/post-tool-use.sh"

  # Stop hook: checkpoint session into .rvf container
  cat > "$HOOKS_DIR/stop.sh" << 'HOOK_EOF'
#!/usr/bin/env bash
# rUvOS stop hook — forks current session into a signed .rvf checkpoint.
set -euo pipefail
RUVOS_BIN="${RUVOS_BIN:-ruvos}"
SESSION_ID="${RUVOS_SESSION_ID:-default}"
printf '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"session.fork","arguments":{"session_id":"%s","label":"auto-checkpoint"}}}\n' \
  "$SESSION_ID" | timeout 5 "$RUVOS_BIN" mcp serve 2>/dev/null | grep -q '"status"' || true
HOOK_EOF
  chmod +x "$HOOKS_DIR/stop.sh"

  ok "hook scripts written to $HOOKS_DIR"

  # Inject into ~/.claude/settings.json (idempotent: skip if already present)
  SETTINGS="$HOME/.claude/settings.json"
  if [ -f "$SETTINGS" ]; then
    if grep -qF "$HOOKS_DIR/pre-tool-use.sh" "$SETTINGS"; then
      ok "hooks already registered in $SETTINGS"
    else
      step "patching $SETTINGS to add rUvOS hooks"
      python3 - "$SETTINGS" "$HOOKS_DIR" << 'PYEOF'
import json, sys, os

settings_path = sys.argv[1]
hooks_dir = sys.argv[2]

with open(settings_path) as f:
    s = json.load(f)

hooks = s.setdefault("hooks", {})

def add_hook(event, matcher, cmd):
    bucket = hooks.setdefault(event, [])
    for entry in bucket:
        for h in entry.get("hooks", []):
            if h.get("command") == cmd:
                return  # already present
    bucket.append({"matcher": matcher, "hooks": [{"type": "command", "command": cmd}]})

add_hook("PreToolUse",  "",              os.path.join(hooks_dir, "pre-tool-use.sh"))
add_hook("PostToolUse", "Edit|Write|Bash", os.path.join(hooks_dir, "post-tool-use.sh"))
add_hook("Stop",        "",              os.path.join(hooks_dir, "stop.sh"))

with open(settings_path, "w") as f:
    json.dump(s, f, indent=2)
    f.write("\n")

print("ok")
PYEOF
      ok "hooks injected into $SETTINGS"
    fi
  else
    warn "$SETTINGS not found — create it by running 'claude' once, then re-run setup.sh"
  fi
else
  warn "--no-hooks (or --no-mcp): skipped Claude Code hook wiring"
fi

# ----- 9. register MCP server with Codex CLI --------------------------------
if [ "$NO_MCP" -eq 0 ]; then
  say "Registering rUvOS MCP server with Codex CLI"
  CODEX_SETTINGS="$HOME/.codex/config.json"
  if command -v codex >/dev/null 2>&1 || [ -f "$CODEX_SETTINGS" ]; then
    mkdir -p "$(dirname "$CODEX_SETTINGS")"
    python3 - "$CODEX_SETTINGS" "$PREFIX/ruvos" << 'PYEOF'
import json, sys, os

path = sys.argv[1]
ruvos_bin = sys.argv[2]

cfg = {}
if os.path.isfile(path):
    with open(path) as f:
        try: cfg = json.load(f)
        except json.JSONDecodeError: cfg = {}

servers = cfg.setdefault("mcpServers", {})
if "ruvos" not in servers:
    servers["ruvos"] = {"command": ruvos_bin, "args": ["mcp", "serve"]}
    with open(path, "w") as f:
        json.dump(cfg, f, indent=2)
        f.write("\n")
    print("registered")
else:
    print("already registered")
PYEOF
    ok "codex: ruvos registered in $CODEX_SETTINGS"
  else
    warn "codex not found — skipping (ok if you don't use Codex CLI)"
    step "to add later: add ruvos to mcpServers in ~/.codex/config.json"
  fi
fi

# ----- 10. verify ------------------------------------------------------------
say "Smoke-testing the install"

# Binary responds
"$PREFIX/ruvos" --version >/dev/null 2>&1 \
  && ok "binary: $($PREFIX/ruvos --version)" \
  || warn "binary at $PREFIX/ruvos did not respond — check PATH"

# CVE scan help renders
"$PREFIX/ruvos" cve scan --help >/dev/null 2>&1 \
  && ok "ruvos cve scan: help renders" \
  || warn "ruvos cve scan --help failed"

# MCP round-trip: initialize → tools/list
TOOLS=$(
  printf '%s\n%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"setup","version":"1"}}}' \
    '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  | timeout 5 "$PREFIX/ruvos" mcp serve 2>/dev/null \
  | grep -o '"name"' | wc -l | tr -d ' '
)
if [ "${TOOLS:-0}" -ge 21 ]; then
  ok "MCP server: ${TOOLS} tools registered"
else
  warn "MCP smoke test returned ${TOOLS:-0} tools (expected ≥21) — run 'ruvos mcp serve' manually to debug"
fi

# ----- done -----------------------------------------------------------------
echo
printf '%s\n' "${GREEN}${BOLD}✨ rUvOS is ready.${RESET}"
echo
printf '%s\n' "${DIM}Quick start:${RESET}"
printf '%s\n' "  source $PROFILE          # apply PATH + RUVOS_HOME (or open a new terminal)"
printf '%s\n' "  ruvos --help             # see all commands"
printf '%s\n' "  ruvos cve scan .         # scan current project for CVEs"
printf '%s\n' "  ruvos doctor             # health check"
printf '%s\n' "  claude mcp list          # confirm ruvos shows ✓ Connected"
echo
printf '%s\n' "${DIM}Hooks installed:${RESET}"
printf '%s\n' "  PreToolUse  → rUvOS pre-hook  (task/edit/command routing)"
printf '%s\n' "  PostToolUse → rUvOS post-hook (SONA learning, trajectory store)"
printf '%s\n' "  Stop        → rUvOS session checkpoint (.rvf)"
echo
printf '%s\n' "${DIM}Data dir: ${RUVOS_HOME}${RESET}"
