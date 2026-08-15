#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=resolve-brain.sh
source "$SCRIPT_DIR/resolve-brain.sh"
BRAIN="$(resolve_fluctlight_brain)"
FLUCTLIGHT="$("$SCRIPT_DIR/fluctlight-bin.sh")"
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
echo "{\"backup\":\"$DEST\",\"timestamp\":\"$STAMP\",\"brain\":\"$BRAIN\"}"

# Retain only the newest N dated backups (default 3).
KEEP="${FLUCTLIGHT_BACKUP_KEEP:-3}"
BACKUP_ROOT="${FLUCTLIGHT_BACKUP_DIR:-$HOME/.fluctlight/backups}"
if [[ -d "$BACKUP_ROOT" && "$KEEP" =~ ^[0-9]+$ ]]; then
  mapfile -t OLD < <(ls -1d "$BACKUP_ROOT"/[0-9]*T*Z 2>/dev/null | sort | head -n -"$KEEP" || true)
  for d in "${OLD[@]:-}"; do
    [[ -n "$d" && -d "$d" ]] || continue
    rm -rf "$d"
  done
fi
