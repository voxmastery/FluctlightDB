#!/usr/bin/env bash
# LongMemEval-S full 500 E2E via Gemini 2.5 Flash (free tier) or Cerebras PayGo.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

LOCK="${LONGMEMEVAL_E2E_LOCK:-/tmp/longmemeval-e2e-500.lock}"
exec 9>"$LOCK"
if ! flock -n 9; then
  echo "Another E2E run holds $LOCK — exit or remove if stale." >&2
  exit 1
fi

export LITELLM_ENV_FILE="${LITELLM_ENV_FILE:-/home/ambugo/litellm/.env}"
if [[ -f "$LITELLM_ENV_FILE" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$LITELLM_ENV_FILE"
  set +a
fi

export FLUCTLIGHT_EMBED_URL="${FLUCTLIGHT_EMBED_URL:-http://127.0.0.1:8794}"
export LONGMEMEVAL_E2E_WORKERS="${LONGMEMEVAL_E2E_WORKERS:-2}"
export LONGMEMEVAL_LLM_BACKEND="${LONGMEMEVAL_LLM_BACKEND:-gemini}"
export LONGMEMEVAL_LLM_TIMEOUT="${LONGMEMEVAL_LLM_TIMEOUT:-180}"

LIMIT="${1:-500}"
BACKEND="${2:-$LONGMEMEVAL_LLM_BACKEND}"
OUT="${3:-benchmarks/results/longmemeval-e2e-v4-mpnet.json}"
CKPT="${4:-benchmarks/results/longmemeval-e2e-v4-mpnet.checkpoint.jsonl}"
LOG="${5:-/tmp/longmemeval-e2e-500.log}"

READER_MODEL="${READER_MODEL:-gemini-2.5-flash}"
JUDGE_MODEL="${JUDGE_MODEL:-gemini-2.5-flash}"
E2E_PROFILE="${LONGMEMEVAL_E2E_PROFILE:-brain}"
EXTRA_ARGS=()
if [[ "$BACKEND" == "cerebras" ]]; then
  READER_MODEL="${READER_MODEL:-gpt-oss-120b}"
  JUDGE_MODEL="${JUDGE_MODEL:-gpt-oss-120b}"
elif [[ "$BACKEND" == "openai" ]]; then
  EXTRA_ARGS=(--e2e-profile "$E2E_PROFILE")
  if [[ "$E2E_PROFILE" == "brain" ]]; then
    READER_MODEL="${READER_MODEL:-gpt-5}"
    JUDGE_MODEL="${JUDGE_MODEL:-gpt-4o-2024-08-06}"
    export LONGMEMEVAL_LLM_TIMEOUT="${LONGMEMEVAL_LLM_TIMEOUT:-360}"
    export LONGMEMEVAL_BRAIN_SLEEP="${LONGMEMEVAL_BRAIN_SLEEP:-2}"
    EXTRA_ARGS+=(--brain-sleep "$LONGMEMEVAL_BRAIN_SLEEP")
  elif [[ "$E2E_PROFILE" == "max" ]]; then
    READER_MODEL="${READER_MODEL:-gpt-5}"
    JUDGE_MODEL="${JUDGE_MODEL:-gpt-4o-2024-08-06}"
    export LONGMEMEVAL_LLM_TIMEOUT="${LONGMEMEVAL_LLM_TIMEOUT:-300}"
  else
    READER_MODEL="${READER_MODEL:-gpt-4o-2024-08-06}"
    JUDGE_MODEL="${JUDGE_MODEL:-gpt-4o-2024-08-06}"
  fi
fi

DATA="${LONGMEMEVAL_DATA:-/tmp/longmemeval/data/longmemeval_s_cleaned.json}"
if [[ ! -f "$DATA" ]]; then
  echo "Dataset missing: $DATA" >&2
  exit 1
fi

echo "E2E limit=$LIMIT backend=$BACKEND workers=$LONGMEMEVAL_E2E_WORKERS log=$LOG"
PYTHONPATH="sdks/python:benchmarks" python3 benchmarks/longmemeval_e2e.py \
  --data "$DATA" \
  --limit "$LIMIT" \
  --top-k 8 \
  --dual-key --pref-facts-key --query-expand \
  --llm-backend "$BACKEND" \
  --reader-model "$READER_MODEL" \
  --judge-model "$JUDGE_MODEL" \
  --workers "$LONGMEMEVAL_E2E_WORKERS" \
  "${EXTRA_ARGS[@]}" \
  --checkpoint "$CKPT" \
  --json-out "$OUT" 2>&1 | tee -a "$LOG"

bash "$ROOT/scripts/finalize-paper-v2-e2e.sh" "$OUT"
