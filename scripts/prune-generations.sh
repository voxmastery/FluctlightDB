#!/usr/bin/env bash
# Keep only the newest N v4 generations for a brain (default: CURRENT + extras up to KEEP).
set -euo pipefail
BRAIN="${1:-${FLUCTLIGHT_BRAIN_PATH:-$HOME/.fluctlight/tenants/serverbrain-v2/brain}}"
KEEP="${FLUCTLIGHT_GENERATION_KEEP:-3}"
GENS="$BRAIN/generations"
CURRENT_FILE="$BRAIN/CURRENT"

[[ -d "$GENS" && -f "$CURRENT_FILE" ]] || exit 0
CURRENT="$(tr -d '[:space:]' < "$CURRENT_FILE")"
[[ -n "$CURRENT" && -d "$GENS/$CURRENT" ]] || exit 1

mapfile -t ALL < <(ls -1 "$GENS" | grep -E '^gen-[0-9]+$' | sort)
((${#ALL[@]} <= KEEP)) && { echo "{\"brain\":\"$BRAIN\",\"current\":\"$CURRENT\",\"kept\":${#ALL[@]},\"removed\":0}"; exit 0; }

KEEP_SET=("$CURRENT")
for ((i=${#ALL[@]}-1; i>=0 && ${#KEEP_SET[@]}<KEEP; i--)); do
  name="${ALL[$i]}"
  [[ " ${KEEP_SET[*]} " == *" $name "* ]] && continue
  KEEP_SET+=("$name")
done

removed=0
for name in "${ALL[@]}"; do
  skip=0
  for k in "${KEEP_SET[@]}"; do
    [[ "$name" == "$k" ]] && skip=1 && break
  done
  ((skip)) && continue
  rm -rf "$GENS/$name"
  removed=$((removed+1))
done
echo "{\"brain\":\"$BRAIN\",\"current\":\"$CURRENT\",\"kept\":${#KEEP_SET[@]},\"removed\":$removed}"
