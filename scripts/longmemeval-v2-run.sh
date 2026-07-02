#!/usr/bin/env bash
# LongMemEval-S v2 — session recall with dual-key + query-expand.
# Usage:
#   ./scripts/longmemeval-v2-run.sh fast          # lexical only (~30 min)
#   ./scripts/longmemeval-v2-run.sh full          # with embeddings (~hours on CPU)
#   ./scripts/longmemeval-v2-run.sh preference    # 30 preference questions only
#   ./scripts/longmemeval-v2-run.sh full-mpnet   # embeddings via multi-qa-mpnet :8794
#   ./scripts/longmemeval-v2-run.sh watch         # wait for preference, then full-mpnet

set -euo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
DATA="${LONGMEMEVAL_DATA:-/tmp/longmemeval/data/longmemeval_s_cleaned.json}"
export PYTHONUNBUFFERED=1
export PYTHONPATH="${REPO}/sdks/python"
export FLUCTLIGHT_EMBED_URL="${FLUCTLIGHT_EMBED_URL:-http://127.0.0.1:8793}"

BENCH=(python3 "${REPO}/benchmarks/longmemeval_bench.py"
  --data "$DATA"
  --mode index
  --granularity session
  --metric session
  --top-k 8
  --dual-key
  --query-expand)

run_fast() {
  echo "[v2] fast (lexical) full 500..."
  "${BENCH[@]}" --fast \
    --checkpoint /tmp/longmemeval-v2-fast.jsonl \
    --json-out "${REPO}/benchmarks/results/longmemeval-session-v2-fast.json" \
    2>&1 | tee /tmp/longmemeval-v2-fast.log
}

run_full() {
  local embed_url="${1:-${FLUCTLIGHT_EMBED_URL:-http://127.0.0.1:8793}}"
  local out="${2:-${REPO}/benchmarks/results/longmemeval-session-v2-2026-07-02.json}"
  local ckpt="${3:-/tmp/longmemeval-v2-checkpoint.jsonl}"
  local log="${4:-/tmp/longmemeval-v2-full.log}"
  echo "[v2] full 500 with embeddings embed_url=$embed_url"
  FLUCTLIGHT_EMBED_URL="$embed_url" "${BENCH[@]}" \
    --checkpoint "$ckpt" \
    --json-out "$out" \
    2>&1 | tee "$log"
}

ensure_mpnet() {
  if curl -sf "http://127.0.0.1:8794/health" >/dev/null 2>&1; then
    echo "[mpnet] already up on :8794"
    return 0
  fi
  echo "[mpnet] starting sentence-transformers/multi-qa-mpnet-base-dot-v1 on :8794..."
  nohup "${REPO}/scripts/start-embed-mpnet.sh" >> /tmp/embed-mpnet-8794.log 2>&1 &
  for _ in $(seq 1 120); do
    if curl -sf "http://127.0.0.1:8794/health" >/dev/null 2>&1; then
      echo "[mpnet] ready"
      return 0
    fi
    sleep 5
  done
  echo "[mpnet] failed to start on :8794" >&2
  return 1
}

run_full_mpnet() {
  ensure_mpnet
  run_full "http://127.0.0.1:8794" \
    "${REPO}/benchmarks/results/longmemeval-session-v2-mpnet.json" \
    /tmp/longmemeval-v2-mpnet.jsonl \
    /tmp/longmemeval-v2-mpnet.log
}

run_preference() {
  echo "[v2] preference slice (30 questions)..."
  "${BENCH[@]}" --type-filter single-session-preference \
    --checkpoint /tmp/lme-pref-v2.jsonl \
    --json-out "${REPO}/benchmarks/results/longmemeval-preference-v2.json" \
    2>&1 | tee /tmp/lme-pref-v2.log
}

watch_and_full() {
  echo "[watch] waiting for preference run (30 lines in checkpoint)..."
  while [[ $(wc -l < /tmp/lme-pref-v2.jsonl 2>/dev/null || echo 0) -lt 30 ]]; do
    n=$(wc -l < /tmp/lme-pref-v2.jsonl 2>/dev/null || echo 0)
    echo "  preference progress: ${n}/30"
    sleep 120
  done
  echo "[watch] preference done; starting full mpnet embed run on :8794..."
  run_full_mpnet
}

case "${1:-fast}" in
  fast) run_fast ;;
  full) run_full ;;
  full-mpnet) run_full_mpnet ;;
  preference) run_preference ;;
  watch) watch_and_full ;;
  *) echo "usage: $0 {fast|full|full-mpnet|preference|watch}"; exit 1 ;;
esac
