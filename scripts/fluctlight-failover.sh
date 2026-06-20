#!/usr/bin/env bash
# Promote local replica brain to primary (disaster recovery).
set -euo pipefail

PRIMARY="${FLUCTLIGHT_PRIMARY_BRAIN:-$HOME/.fluctlight/tenants/default/brain}"
REPLICA="${FLUCTLIGHT_REPLICA_DIR:-$HOME/.fluctlight/replica}"
REPLICA_BRAIN="$REPLICA/brain"
FLUCTLIGHT="${FLUCTLIGHT_BIN:-$HOME/fluctlightdb/target/release/fluctlight}"
STAMP=$(date -u +%Y%m%dT%H%M%SZ)

if [[ ! -f "$REPLICA_BRAIN/manifest.json" ]]; then
  echo "replica brain missing: $REPLICA_BRAIN/manifest.json" >&2
  exit 1
fi

echo "Stopping fluctlight-serve..."
sudo systemctl stop fluctlight-serve || true

if [[ -d "$PRIMARY" ]]; then
  mv "$PRIMARY" "${PRIMARY}.pre-failover.${STAMP}"
fi

mkdir -p "$(dirname "$PRIMARY")"
cp -a "$REPLICA_BRAIN" "$PRIMARY"

"$FLUCTLIGHT" verify --path "$PRIMARY"

echo "Restarting fluctlight-serve..."
sudo systemctl start fluctlight-serve

echo "Failover complete. Previous primary saved to ${PRIMARY}.pre-failover.${STAMP}"
