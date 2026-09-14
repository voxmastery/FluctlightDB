#!/usr/bin/env bash
# Fail a Fluctlight serve cutover before it wedges the live flock.
# Usage: FLUCTLIGHT_BIN=... FLUCTLIGHT_BRAIN_PATH=... ./scripts/preflight-serve.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=resolve-brain.sh
source "$ROOT/scripts/resolve-brain.sh"

BIN="${FLUCTLIGHT_BIN:-}"
BRAIN="$(resolve_fluctlight_brain)"
err() { echo "preflight: $*" >&2; exit 1; }
warn() { echo "preflight warn: $*" >&2; }

[[ -n "$BIN" ]] || err "FLUCTLIGHT_BIN is unset — pin an exact binary, do not glob"
[[ -x "$BIN" ]] || err "FLUCTLIGHT_BIN not executable: $BIN"
[[ -d "$BRAIN" ]] || err "brain path is not a directory: $BRAIN"

if timeout 3 "$BIN" help >/dev/null 2>&1 || timeout 3 "$BIN" >/dev/null 2>&1; then
  echo "preflight: binary answers help  $BIN"
else
  err "binary did not print help within 3s (do not use --version; it hangs on some builds)"
fi

CURRENT="$BRAIN/CURRENT"
if [[ -f "$CURRENT" ]]; then
  GEN_NAME="$(tr -d '[:space:]' <"$CURRENT")"
  GEN="$BRAIN/generations/$GEN_NAME"
  [[ -d "$GEN" ]] || err "CURRENT=$GEN_NAME but $GEN is missing"
  echo "preflight: CURRENT $GEN_NAME"
  TAU="$GEN/tau.seg"
  if [[ -f "$TAU" ]]; then
    TAU_MB="$(du -m "$TAU" | awk '{print $1}')"
    echo "preflight: tau.seg ${TAU_MB}M"
    if [[ "$TAU_MB" -ge 80 && -z "${FLUCTLIGHT_ALLOW_SLOW_OPEN:-}" ]]; then
      err "tau.seg is ${TAU_MB}M — open can take tens of minutes. Set FLUCTLIGHT_ALLOW_SLOW_OPEN=1 or park tau on a COPY first"
    fi
  fi
  if [[ -f "$GEN/manifest.json" ]]; then
    python3 -c "import json,sys; print('preflight: generation wal_seq='+str(json.load(open(sys.argv[1])).get('wal_seq')))" "$GEN/manifest.json"
  fi
else
  warn "no CURRENT — serve will load root *.seg (stale vs generations/)"
  if [[ -d "$BRAIN/wal" ]]; then
    err "root snapshot + wal/ present without CURRENT — 0.5.21+ raises WAL sequence gap. Park wal/ or restore CURRENT"
  fi
fi

if [[ -z "${FLUCTLIGHT_BIN:-}" ]]; then
  err "unreachable"
fi
echo "preflight: ok  bin=$BIN  brain=$BRAIN"
