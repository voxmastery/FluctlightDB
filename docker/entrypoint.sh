#!/bin/sh
set -eu

export HOME="${FLUCTLIGHT_HOME:-/data}"
TENANT="${FLUCTLIGHT_TENANT_ID:-default}"
BRAIN="${FLUCTLIGHT_BRAIN_PATH:-$HOME/.fluctlight/tenants/$TENANT/brain}"
ADDR="${FLUCTLIGHT_SERVE_ADDR:-0.0.0.0:8792}"

export FLUCTLIGHT_STORAGE="${FLUCTLIGHT_STORAGE:-v4}"

mkdir -p "$(dirname "$BRAIN")"

if [ ! -f "$BRAIN/manifest.json" ]; then
  echo "fluctlight: initializing brain at $BRAIN"
  fluctlight tenant create "$TENANT" >/dev/null
fi

if [ -z "${FLUCTLIGHT_API_KEYS:-}" ]; then
  echo "fluctlight: set FLUCTLIGHT_API_KEYS (tenant:key:role) for non-localhost serve" >&2
  echo "  example: FLUCTLIGHT_API_KEYS=default:change-me-in-production:write" >&2
  exit 1
fi

exec fluctlight serve --addr "$ADDR" --path "$BRAIN"
