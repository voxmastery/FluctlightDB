#!/usr/bin/env bash
# Resolve the live Fluctlight brain path for ops scripts.
# Prefer explicit env, then the default tenant.
resolve_fluctlight_brain() {
  if [[ -n "${FLUCTLIGHT_BRAIN_PATH:-}" ]]; then
    printf '%s\n' "$FLUCTLIGHT_BRAIN_PATH"
    return 0
  fi
  if [[ -n "${FLUCTLIGHT_PRIMARY_BRAIN:-}" ]]; then
    printf '%s\n' "$FLUCTLIGHT_PRIMARY_BRAIN"
    return 0
  fi
  local home="${HOME:?HOME must be set}"
  local cand="$home/.fluctlight/tenants/default/brain"
  printf '%s\n' "$cand"
}
