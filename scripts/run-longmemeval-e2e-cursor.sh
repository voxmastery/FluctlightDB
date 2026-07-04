#!/usr/bin/env bash
# LongMemEval E2E via Cursor Cloud Agents API (Auto + CURSOR_API_KEY from serverbrain).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export CURSOR_ENV_FILE="${CURSOR_ENV_FILE:-/opt/ambugo/serverbrain/.env}"
if [[ -f "$CURSOR_ENV_FILE" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$CURSOR_ENV_FILE"
  set +a
fi

export FLUCTLIGHT_EMBED_URL="${FLUCTLIGHT_EMBED_URL:-http://127.0.0.1:8794}"

LIMIT="${1:-50}"
OUT="${2:-benchmarks/results/longmemeval-e2e-cursor-auto-${LIMIT}.json}"
CKPT="${3:-benchmarks/results/longmemeval-e2e-cursor-auto-${LIMIT}.checkpoint.jsonl}"

if [[ -z "${CURSOR_API_KEY:-}" ]]; then
  echo "CURSOR_API_KEY not set (try CURSOR_ENV_FILE=$CURSOR_ENV_FILE)" >&2
  exit 1
fi

DATA="${LONGMEMEVAL_DATA:-/tmp/longmemeval/data/longmemeval_s_cleaned.json}"
if [[ ! -f "$DATA" ]]; then
  echo "Dataset missing: $DATA" >&2
  exit 1
fi

PYTHONPATH="sdks/python:benchmarks" python3 benchmarks/longmemeval_e2e.py \
  --data "$DATA" \
  --limit "$LIMIT" \
  --top-k 8 \
  --dual-key --pref-facts-key --query-expand \
  --reader-model auto \
  --judge-model auto \
  --cursor-timeout "${CURSOR_API_TIMEOUT:-300}" \
  --checkpoint "$CKPT" \
  --json-out "$OUT"
