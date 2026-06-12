#!/usr/bin/env bash
# Live multi-agent execution test via OpenRouter.
#
# Proves that ruvos agents perform REAL LLM inference (not placeholder
# artifacts) using your OpenRouter API key. Forces the OpenRouter route via an
# isolated llm.json (so a local claude/gemini/codex CLI on PATH doesn't win
# routing priority), then spawns multiple agents + runs an orchestration
# pipeline and checks each used the OpenRouter provider with real output.
#
# Usage:
#   1. Put your key in .env:  OPENROUTER_API_KEY=sk-or-...
#   2. ./scripts/test-multiagent-openrouter.sh
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$(pwd)"
BIN="$ROOT/target/release/ruvos"

# Load .env (key + TEST_MODEL). Real env wins — an already-set/exported var is
# never overridden by .env (same precedence as ruvos's own .env loader).
if [[ -f .env ]]; then
    while IFS= read -r line; do
        line="${line#"${line%%[![:space:]]*}"}"   # ltrim
        [[ -z "$line" || "$line" == \#* ]] && continue
        line="${line#export }"
        [[ "$line" != *=* ]] && continue
        key="${line%%=*}"; key="${key//[[:space:]]/}"
        val="${line#*=}"
        val="${val#"${val%%[![:space:]]*}"}"; val="${val%"${val##*[![:space:]]}"}"  # trim
        val="${val%\"}"; val="${val#\"}"; val="${val%\'}"; val="${val#\'}"           # unquote
        [[ -z "$key" ]] && continue
        [[ -z "${!key:-}" ]] && export "$key=$val"
    done < .env
fi
MODEL="${TEST_MODEL:-openai/gpt-4o-mini}"

if [[ ! -x "$BIN" ]]; then
    echo "Building release binary…"
    cargo build --release -p ruvos-cli --jobs 4
fi

if [[ -z "${OPENROUTER_API_KEY:-}" ]]; then
    echo "✗ OPENROUTER_API_KEY is empty."
    echo "  Paste your key into $ROOT/.env  (OPENROUTER_API_KEY=sk-or-...) and re-run."
    exit 2
fi
command -v curl >/dev/null || { echo "✗ curl not found (the OpenRouter route shells out to curl)"; exit 3; }

# Isolated data dir + llm.json forcing the OpenRouter route with $MODEL.
WORK="$(mktemp -d /tmp/ruvos-multiagent.XXXXXX)"
export RUVOS_HOME="$WORK/.ruvos"
mkdir -p "$RUVOS_HOME"
cat > "$RUVOS_HOME/llm.json" <<JSON
{ "routing": { "priority": ["openrouter"] },
  "openrouter": { "default_model": "$MODEL" } }
JSON

echo "Model:      $MODEL"
echo "RUVOS_HOME: $RUVOS_HOME"
echo "Running 2 agent spawns + 1 orchestration pipeline through OpenRouter…"
echo

hs='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"multiagent-test","version":"1"}}}'
init='{"jsonrpc":"2.0","method":"notifications/initialized"}'
spawn_coder='{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ruvos_agent_spawn","arguments":{"archetype":"coder","prompt":"Write a one-line Rust function that returns the nth Fibonacci number. Reply with only the code.","model":"openrouter"}}}'
spawn_tester='{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"ruvos_agent_spawn","arguments":{"archetype":"tester","prompt":"Suggest one edge-case test for a Fibonacci function. One sentence.","model":"openrouter"}}}'
orchestrate='{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"ruvos_orchestrate_run","arguments":{"template":"feature","task":"add a /health endpoint"}}}'

OUT="$WORK/out.jsonl"
printf '%s\n' "$hs" "$init" "$spawn_coder" "$spawn_tester" "$orchestrate" \
    | timeout 180 "$BIN" mcp serve 2>"$WORK/serve.err" > "$OUT" || true

python3 - "$OUT" "$RUVOS_HOME" <<'PY'
import json, sys, glob, os
out, rhome = sys.argv[1], sys.argv[2]
rows = {}
for line in open(out):
    line=line.strip()
    if not line: continue
    try: m=json.loads(line)
    except: continue
    if "id" in m: rows[m["id"]]=m

def field(m, *path):
    sc = (m.get("result") or {}).get("structuredContent") or {}
    cur = sc
    for p in path:
        if isinstance(cur, dict): cur = cur.get(p)
    return cur

real = 0
for rid, label in [(2,"agent_spawn: coder"), (3,"agent_spawn: tester")]:
    m = rows.get(rid)
    if not m: print(f"  {label}: NO RESPONSE"); continue
    result = field(m,"result") or field(m,"summary") or ""
    success = field(m,"success")
    used_or = "openrouter" in str(result).lower()
    real += 1 if used_or else 0
    print(f"  {label}: success={success}  provider={'openrouter' if used_or else 'fallback/placeholder'}")
    print(f"    result: {str(result)[:120]}")

m = rows.get(4)
if m:
    steps = field(m,"steps")
    status = field(m,"status")
    print(f"  orchestrate_run(feature): status={status}  steps={steps}")

print()
# Show real LLM artifacts written to disk
arts = sorted(glob.glob(os.path.join(rhome, "agents", "*", "output.md")))
print(f"  agent artifacts on disk: {len(arts)}")
for a in arts[:2]:
    body = open(a).read().strip().replace("\n"," ")
    print(f"    {os.path.basename(os.path.dirname(a))}: {body[:140]}")

print()
if real >= 1:
    print("✓ PASS — at least one agent performed real OpenRouter inference.")
    sys.exit(0)
else:
    print("✗ FAIL — agents fell back to placeholders. Check the key/model and:")
    print("    tail:", open(sys.argv[1].replace('out.jsonl','serve.err')).read()[-400:] if os.path.exists(sys.argv[1].replace('out.jsonl','serve.err')) else "(no stderr)")
    sys.exit(1)
PY
RC=$?
echo
echo "(artifacts in $RUVOS_HOME — remove with: rm -rf $WORK)"
exit $RC
