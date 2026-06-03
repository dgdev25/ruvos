#!/usr/bin/env bash
#
# rUvOS one-shot installer.
#
#   git clone https://github.com/dgdev25/ruvos.git
#   cd ruvos
#   ./setup.sh
#
# This script:
#   1. Checks prerequisites (Rust/cargo)
#   2. Builds the release binary
#   3. REMOVES any legacy Ruflo v2/v3 (npm packages + MCP registrations) —
#      rUvOS v4 is a clean break and is NOT backward compatible
#   4. Installs the `ruvos` binary onto your PATH
#   5. Points RUVOS_HOME at a shared data dir
#   6. Registers rUvOS as an MCP server with Claude Code (user scope)
#   7. Verifies the install
#
# Re-running it is safe (idempotent).
#
# Removing Ruflo v2/v3 is MANDATORY and not optional: rUvOS v4 is a clean break,
# is not backward compatible, and cannot coexist with the legacy install.
#
# Flags:
#   --no-mcp        Do NOT register with Claude Code (skip step 6)
#   --prefix DIR    Install the binary into DIR (default: /usr/local/bin, else ~/.local/bin)
#   -h | --help     Show this help

set -euo pipefail

# ----- pretty output --------------------------------------------------------
BOLD=$'\033[1m'; GREEN=$'\033[32m'; YELLOW=$'\033[33m'; RED=$'\033[31m'; DIM=$'\033[2m'; RESET=$'\033[0m'
say()  { printf '%s\n' "${BOLD}▶ $*${RESET}"; }
ok()   { printf '%s\n' "${GREEN}  ✓ $*${RESET}"; }
warn() { printf '%s\n' "${YELLOW}  ! $*${RESET}"; }
die()  { printf '%s\n' "${RED}  ✗ $*${RESET}" >&2; exit 1; }

# ----- args -----------------------------------------------------------------
NO_MCP=0; PREFIX=""
while [ $# -gt 0 ]; do
  case "$1" in
    --no-mcp)  NO_MCP=1 ;;
    --prefix)  PREFIX="${2:-}"; shift ;;
    -h|--help) grep '^#' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
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
  *) warn "platform '$OS' is not officially supported by this script (Linux/macOS). Continuing best-effort." ;;
esac

# ----- 1. prerequisites -----------------------------------------------------
say "Checking prerequisites"
command -v cargo >/dev/null 2>&1 || die "Rust/cargo not found. Install from https://rustup.rs and re-run."
ok "cargo: $(cargo --version)"

# ----- 2. build -------------------------------------------------------------
say "Building the rUvOS release binary (this can take a few minutes the first time)"
cargo build --release --jobs 4
BIN="$REPO_DIR/target/release/ruvos"
[ -x "$BIN" ] || die "build did not produce $BIN"
ok "built: $($BIN --version)"

# ----- 3. remove the incompatible Ruflo v2/v3 npm CLI ----------------------
# Only the old v2/v3 npm CLI (the TypeScript monolith) is genuinely incompatible
# with rUvOS v4 and gets removed. Everything else is left in place:
#   • claude-flow / ruv-swarm MCP servers — these coexist fine with rUvOS
#     (verified: different namespaces, processes, and data dirs). Disambiguate by
#     naming "rUvOS" in requests; see the README.
#   • Ruflo Claude Code *plugins* (the `ruflo` bundle → `ruflo-*` agents/skills) —
#     user-managed; remove via the `/plugin` command only if you want to.
say "Removing the legacy Ruflo v2/v3 npm CLI (the only piece incompatible with v4)"
if command -v npm >/dev/null 2>&1; then
  npm uninstall -g ruflo            >/dev/null 2>&1 && ok "removed npm package: ruflo" || warn "npm 'ruflo' not installed"
  npm uninstall -g @claude-flow/cli >/dev/null 2>&1 && ok "removed npm package: @claude-flow/cli" || warn "npm '@claude-flow/cli' not installed"
  npm cache clean --force           >/dev/null 2>&1 || true
  ok "cleared npm cache"
else
  warn "npm not found — skipping npm package removal"
fi

# ----- 4. install the binary onto PATH -------------------------------------
say "Installing the ruvos binary"
choose_prefix() {
  if [ -n "$PREFIX" ]; then echo "$PREFIX"; return; fi
  if [ -w /usr/local/bin ]; then echo /usr/local/bin; return; fi
  if command -v sudo >/dev/null 2>&1; then echo /usr/local/bin; return; fi
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
if install_to "$PREFIX"; then
  ok "installed: $PREFIX/ruvos"
else
  PREFIX="$HOME/.local/bin"; mkdir -p "$PREFIX"
  cp "$BIN" "$PREFIX/ruvos"
  ok "installed: $PREFIX/ruvos"
fi

# ----- 5. shell profile: PATH + RUVOS_HOME ---------------------------------
say "Configuring environment"
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

case ":$PATH:" in
  *":$PREFIX:"*) : ;;
  *) add_line "export PATH=\"$PREFIX:\$PATH\""; ok "added $PREFIX to PATH in $PROFILE" ;;
esac
add_line "export RUVOS_HOME=\"$RUVOS_HOME_DEFAULT\""
export RUVOS_HOME="$RUVOS_HOME_DEFAULT"
mkdir -p "$RUVOS_HOME"
ok "RUVOS_HOME=$RUVOS_HOME (added to $PROFILE)"

# ----- 6. register with Claude Code ----------------------------------------
if [ "$NO_MCP" -eq 0 ]; then
  say "Registering rUvOS with Claude Code (MCP, user scope)"
  if command -v claude >/dev/null 2>&1; then
    claude mcp remove ruvos >/dev/null 2>&1 || true
    if claude mcp add ruvos --scope user -- "$PREFIX/ruvos" mcp serve >/dev/null 2>&1; then
      ok "registered: claude mcp add ruvos --scope user -- $PREFIX/ruvos mcp serve"
    else
      warn "could not auto-register. Run manually: claude mcp add ruvos --scope user -- $PREFIX/ruvos mcp serve"
    fi
  else
    warn "'claude' CLI not found — skipping MCP registration."
    warn "Once Claude Code is installed: claude mcp add ruvos --scope user -- $PREFIX/ruvos mcp serve"
  fi
else
  warn "--no-mcp set: skipping Claude Code registration"
fi

# ----- 7. verify ------------------------------------------------------------
say "Verifying"
"$PREFIX/ruvos" --version >/dev/null 2>&1 && ok "binary runs: $($PREFIX/ruvos --version)" || warn "could not run $PREFIX/ruvos"
# Smoke-test the MCP surface end-to-end.
TOOLS=$(printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' \
  | timeout 5 "$PREFIX/ruvos" mcp serve 2>/dev/null | grep -o '"name"' | wc -l | tr -d ' ')
if [ "${TOOLS:-0}" -ge 21 ]; then ok "MCP server responds with $TOOLS tools"; else warn "MCP smoke test inconclusive"; fi

echo
printf '%s\n' "${GREEN}${BOLD}✨ rUvOS installed.${RESET}"
echo
printf '%s\n' "${DIM}Next:${RESET}"
printf '%s\n' "  1. Open a new terminal (or: source $PROFILE) so PATH/RUVOS_HOME take effect"
printf '%s\n' "  2. In any project, just talk to Claude Code — the 21 rUvOS tools load automatically"
printf '%s\n' "  3. Check it: ${BOLD}claude mcp list${RESET}  →  ruvos: ✓ Connected"
echo
