#!/usr/bin/env bash
set -euo pipefail
BRAIN="${FLUCTLIGHT_BRAIN_PATH:-$HOME/.fluctlight/tenants/default/brain}"
FLUCTLIGHT="$("$(dirname "$0")/fluctlight-bin.sh")"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
DEST="${FLUCTLIGHT_BACKUP_DIR:-$HOME/.fluctlight/backups}/$STAMP"
mkdir -p "$DEST"
if [[ -d "$BRAIN" ]]; then
  cp -a "$BRAIN" "$DEST/brain_v4"
  cp -a "$BRAIN/wal" "$DEST/wal" 2>/dev/null || true
else
  cp -a "$BRAIN" "$DEST/" 2>/dev/null || true
  cp -a "${BRAIN}.wal"* "$DEST/" 2>/dev/null || true
fi
if [[ -x "$FLUCTLIGHT" ]]; then
  "$FLUCTLIGHT" verify --path "$BRAIN" >"$DEST/verify.json" 2>&1 || true
  "$FLUCTLIGHT" export-raw "$DEST/brain-raw.json" --path "$BRAIN" 2>/dev/null || true
fi
echo "{\"backup\":\"$DEST\",\"timestamp\":\"$STAMP\"}"
