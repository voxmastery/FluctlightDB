#!/usr/bin/env bash
# Resolve the live Fluctlight brain path for ops scripts.
# Prefer explicit env, then serverbrain-v2 (live), then default tenant.
resolve_fluctlight_brain() {
  if [[ -n "${FLUCTLIGHT_BRAIN_PATH:-}" ]]; then
    printf '%s\n' "$FLUCTLIGHT_BRAIN_PATH"
    return 0
  fi
  if [[ -n "${FLUCTLIGHT_PRIMARY_BRAIN:-}" ]]; then
    printf '%s\n' "$FLUCTLIGHT_PRIMARY_BRAIN"
    return 0
  fi
  local home="${HOME:-/home/ambugo}"
  local cand
  for cand in \
    "$home/.fluctlight/tenants/serverbrain-v2/brain" \
    "$home/.fluctlight/tenants/default/brain"
  do
    if [[ -d "$cand" || -f "$cand" ]]; then
      printf '%s\n' "$cand"
      return 0
    fi
  done
  printf '%s\n' "$home/.fluctlight/tenants/serverbrain-v2/brain"
}
