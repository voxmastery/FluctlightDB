#!/usr/bin/env bash
# Weekly DR drill — verify primary brain; alert if corrupt (exit 1).
set -euo pipefail
BRAIN="${FLUCTLIGHT_PRIMARY_BRAIN:-$HOME/.fluctlight/tenants/default/brain}"
FLUCTLIGHT="${FLUCTLIGHT_BIN:-$HOME/fluctlightdb/target/release/fluctlight}"
OUT=$("$FLUCTLIGHT" verify --path "$BRAIN")
OK=$(echo "$OUT" | grep -o '"ok":[^,]*' | head -1 || true)
if echo "$OUT" | grep -q '"ok": true'; then
  echo "drill ok: $BRAIN"
  exit 0
fi
echo "DRILL FAILED: $OUT" >&2
exit 1
