#!/usr/bin/env bash
# Weekly DR drill — verify primary brain; alert if corrupt (exit 1).
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=resolve-brain.sh
source "$SCRIPT_DIR/resolve-brain.sh"
BRAIN="${FLUCTLIGHT_PRIMARY_BRAIN:-$(resolve_fluctlight_brain)}"
FLUCTLIGHT="$("$SCRIPT_DIR/fluctlight-bin.sh")"
OUT=$("$FLUCTLIGHT" verify --path "$BRAIN")
if echo "$OUT" | grep -q '"ok": true'; then
  echo "drill ok: $BRAIN"
  exit 0
fi
echo "DRILL FAILED ($BRAIN): $OUT" >&2
exit 1
