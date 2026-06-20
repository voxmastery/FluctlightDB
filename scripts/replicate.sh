#!/usr/bin/env bash
# WAL + snapshot replication loop (Phase 5).
set -euo pipefail
PRIMARY="${FLUCTLIGHT_PRIMARY_BRAIN:-$HOME/.fluctlight/tenants/default/brain}"
REPLICA="${FLUCTLIGHT_REPLICA_DIR:-$HOME/.fluctlight/replica}"
INTERVAL="${FLUCTLIGHT_REPLICA_INTERVAL_SEC:-2}"
FLUCTLIGHT="${FLUCTLIGHT_BIN:-$HOME/fluctlightdb/target/release/fluctlight}"

if [[ -x "$FLUCTLIGHT" ]]; then
  exec "$FLUCTLIGHT" replicate --primary "$PRIMARY" --replica "$REPLICA" --interval "$INTERVAL"
fi

mkdir -p "$REPLICA/wal"
while true; do
  if [[ -d "$PRIMARY" ]]; then
    rsync -a --delete "$PRIMARY/" "$REPLICA/brain/"
  elif [[ -f "$PRIMARY" ]]; then
    cp -a "$PRIMARY" "$REPLICA/brain.flct"
  fi
  for seg in "${PRIMARY%/*}/"*.flct.wal* "$PRIMARY/wal/"*; do
    [[ -e "$seg" ]] || continue
    cp -a "$seg" "$REPLICA/wal/" 2>/dev/null || true
  done
  sleep "$INTERVAL"
done
