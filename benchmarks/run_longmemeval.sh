#!/usr/bin/env bash
# Full LongMemEval-S run (500 questions). Resume-safe via checkpoint JSONL.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DATA="${LONGMEMEVAL_DATA:-/tmp/longmemeval/data/longmemeval_s_cleaned.json}"
MODE="${MODE:-index}"
TOP_K="${TOP_K:-8}"
CHECKPOINT="${CHECKPOINT:-/tmp/longmemeval-checkpoint.jsonl}"
OUT="${JSON_OUT:-$ROOT/benchmarks/results/longmemeval-$(date +%Y-%m-%d).json}"

export OMP_NUM_THREADS="${OMP_NUM_THREADS:-2}"
export OPENBLAS_NUM_THREADS="${OPENBLAS_NUM_THREADS:-2}"
export PYTHONPATH="$ROOT/sdks/python${PYTHONPATH:+:$PYTHONPATH}"

cd "$ROOT"
exec python3 benchmarks/longmemeval_bench.py \
  --data "$DATA" \
  --mode "$MODE" \
  --top-k "$TOP_K" \
  --checkpoint "$CHECKPOINT" \
  --json-out "$OUT" \
  "$@"
