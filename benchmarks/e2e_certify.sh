#!/usr/bin/env bash
# LongMemEval E2E certification — run when OPENAI_API_KEY or GEMINI_API_KEY is set.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
PROFILE="${E2E_PROFILE:-paper}"
OUT="${REPO}/benchmarks/results/e2e-cert-${PROFILE}-v2-$(date +%Y-%m-%d).json"
CHECKPOINT="${REPO}/benchmarks/results/e2e-cert-${PROFILE}-v2-$(date +%Y-%m-%d).checkpoint.jsonl"
LOCK="${REPO}/benchmarks/results/.e2e-cert-${PROFILE}.lock"
BACKEND="${E2E_BACKEND:-openai}"
LIMIT="${E2E_LIMIT:-500}"
WORKERS="${E2E_WORKERS:-1}"

export PYTHONUNBUFFERED=1

echo "FluctlightDB E2E certification"
echo "  profile=$PROFILE backend=$BACKEND limit=$LIMIT"
echo "  workers=$WORKERS"
echo "  output=$OUT"

cd "$REPO"
export PYTHONPATH="${REPO}/sdks/python:${REPO}/benchmarks:${PYTHONPATH:-}"

exec 9>"$LOCK"
if ! flock -n 9; then
  echo "ERROR: another E2E certification is already running (lock: $LOCK)"
  exit 1
fi

python3 benchmarks/e2e_preflight.py || exit 1
echo ""

if [[ "${E2E_SKIP_VALIDATE:-0}" != "1" ]]; then
  echo "Running validation gate (must hit >=98% on held-out set)..."
  python3 benchmarks/e2e_validate_gate.py --llm-backend "$BACKEND" || exit 1
  echo ""
fi

# Load GEMINI_API_KEY into the shell from litellm .env if missing
if [[ -z "${GEMINI_API_KEY:-}" && -z "${OPENAI_API_KEY:-}" ]]; then
  for envfile in /home/ambugo/litellm/.env "${HOME}/.env"; do
    if [[ -f "$envfile" ]]; then
      while IFS= read -r line || [[ -n "$line" ]]; do
        line="${line%%#*}"
        line="$(echo "$line" | xargs)"
        [[ -z "$line" || "$line" != *=* ]] && continue
        key="${line%%=*}"
        val="${line#*=}"
        val="${val%\"}"; val="${val#\"}"
        val="${val%\'}"; val="${val#\'}"
        if [[ "$key" == "GEMINI_API_KEY" || "$key" == "OPENAI_API_KEY" ]] && [[ -z "${!key:-}" ]]; then
          export "$key=$val"
        fi
      done < "$envfile"
    fi
  done
fi

if [[ -z "${OPENAI_API_KEY:-}" && -z "${GEMINI_API_KEY:-}" ]]; then
  echo "SKIP: set OPENAI_API_KEY or GEMINI_API_KEY for live E2E certification"
  echo "Retrieval-only numbers are already frozen in benchmarks/results/"
  exit 0
fi

python3 benchmarks/longmemeval_e2e.py \
  --e2e-profile "$PROFILE" \
  --llm-backend "$BACKEND" \
  --limit "$LIMIT" \
  --workers "$WORKERS" \
  --json-out "$OUT" \
  --checkpoint "$CHECKPOINT"

echo "Certification complete: $OUT"
python3 - <<PY
import json
from pathlib import Path
out = Path("$OUT")
data = json.loads(out.read_text())
summary = data.get("summary", data)
acc = summary.get("overall_accuracy") or summary.get("accuracy")
rec = summary.get("session_recall_at_k")
print(f"E2E QA accuracy: {acc}")
print(f"Session recall@k: {rec}")
PY
