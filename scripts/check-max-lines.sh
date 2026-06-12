#!/usr/bin/env bash
# Ratcheted file-size gate (ADR-037).
#
# Every .rs file under crates/ must stay at or below MAX_LINES, except the
# grandfathered files below, which are frozen at their size when this gate
# landed. A grandfathered file may shrink (lower the cap when it does!) but
# may not grow. New files get no exceptions.
#
# Run locally with: just max-lines
set -euo pipefail
cd "$(dirname "$0")/.."

MAX_LINES=500

# path:cap — caps are the line counts at the time ADR-037 landed (2026-06-12).
# Ratchet rule: when you shrink one of these files, lower its cap to match.
GRANDFATHERED="
crates/ruvos-mcp/src/tools/swarm.rs:1992
crates/ruvos-mcp/src/tools/agent_exec.rs:1663
crates/ruvos-mcp/src/tools/orchestrate.rs:1285
crates/ruvos-mcp/src/tools/memory.rs:1259
crates/ruvos-cli/src/commands/skills.rs:1148
crates/ruvos-mcp/src/tools/gov.rs:987
crates/ruvos-mcp/src/tools/intel.rs:934
crates/ruvos-mcp/src/runtime.rs:768
crates/ruvos-mcp/src/tools/hooks.rs:754
crates/ruvos-mcp/src/swarm.rs:737
crates/ruvos-mcp/src/tools/mod.rs:676
crates/ruvos-mcp/src/tools/relay.rs:594
crates/ruvos-mcp/src/math.rs:566
crates/ruvos-mcp/src/relay.rs:537
crates/ruvos-mcp/src/tools/agent/mod.rs:533
"

cap_for() {
    local file="$1"
    local entry
    entry=$(echo "$GRANDFATHERED" | grep -F "${file}:" || true)
    if [[ -n "$entry" ]]; then
        echo "${entry##*:}"
    else
        echo "$MAX_LINES"
    fi
}

fail=0
while IFS= read -r f; do
    lines=$(wc -l <"$f")
    cap=$(cap_for "$f")
    if ((lines > cap)); then
        echo "FAIL: $f is $lines lines (cap: $cap)"
        fail=1
    fi
done < <(find crates -name '*.rs' | sort)

if ((fail)); then
    echo ""
    echo "File-size gate failed (ADR-037). Split the file, or — only for a"
    echo "grandfathered file that genuinely must grow — raise its cap via ADR."
    exit 1
fi
echo "max-lines: OK (cap ${MAX_LINES}, $(echo "$GRANDFATHERED" | grep -c ':' ) grandfathered)"
