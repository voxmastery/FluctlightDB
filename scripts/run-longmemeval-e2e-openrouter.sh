#!/usr/bin/env bash
# Run LongMemEval E2E locally using OpenRouter (gpt-4o reader + judge).
# Note: CURSOR_API_KEY (crsr_*) is Cursor Agent SDK only — not OpenAI chat API.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [[ -f /home/ambugo/ambugo-copilot/.env ]]; then
  set -a
  # shellcheck disable=SC1091
  source /home/ambugo/ambugo-copilot/.env
  set +a
fi

export OPENAI_API_KEY="${OPENAI_API_KEY:-${OPENROUTER_API_KEY:-}}"
export OPENAI_BASE_URL="${OPENAI_BASE_URL:-https://openrouter.ai/api/v1}"
export FLUCTLIGHT_EMBED_URL="${FLUCTLIGHT_EMBED_URL:-http://127.0.0.1:8794}"

LIMIT="${1:-50}"
OUT="${2:-benchmarks/results/longmemeval-e2e-v4-mpnet-${LIMIT}.json}"
CKPT="${3:-benchmarks/results/longmemeval-e2e-v4-mpnet-${LIMIT}.checkpoint.jsonl}"

if [[ -z "${OPENAI_API_KEY}" ]]; then
  echo "Need OPENROUTER_API_KEY or OPENAI_API_KEY in env" >&2
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
  --reader-model "openai/gpt-4o-2024-08-06" \
  --judge-model "openai/gpt-4o-2024-08-06" \
  --checkpoint "$CKPT" \
  --json-out "$OUT"
