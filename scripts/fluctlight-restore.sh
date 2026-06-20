#!/usr/bin/env bash
set -euo pipefail
BACKUP="${1:?usage: fluctlight-restore.sh BACKUP_DIR}"
BRAIN="${FLUCTLIGHT_BRAIN_PATH:-$HOME/.fluctlight/serverbrain.flct}"
echo "Stopping fluctlight-serve (if running)..."
systemctl stop fluctlight-serve 2>/dev/null || true
TMP="${BRAIN}.restore.tmp"
cp -a "$BACKUP/$(basename "$BRAIN")" "$TMP"
mv -f "$TMP" "$BRAIN"
rm -f "${BRAIN}.wal"* 2>/dev/null || true
for wal in "$BACKUP"/"${BRAIN##*/}.wal"*; do
  [[ -e "$wal" ]] && cp -a "$wal" "$(dirname "$BRAIN")/"
done
echo "Restored from $BACKUP"
systemctl start fluctlight-serve 2>/dev/null || echo "Start fluctlight-serve manually"
