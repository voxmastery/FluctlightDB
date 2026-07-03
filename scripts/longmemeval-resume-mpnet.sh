#!/usr/bin/env bash
# After fast v2 completes, resume full 500 mpnet embed benchmark.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
FAST_CKPT=/tmp/longmemeval-v2-fast.jsonl
MPNET_CKPT=/tmp/longmemeval-v2-mpnet.jsonl

echo "[resume] waiting for fast run (500 lines)..."
while [[ $(wc -l < "$FAST_CKPT" 2>/dev/null || echo 0) -lt 500 ]]; do
  n=$(wc -l < "$FAST_CKPT" 2>/dev/null || echo 0)
  rate=$(tail -1 /tmp/longmemeval-v2-fast.log 2>/dev/null | grep -oP 'session_recall_at_k@\d+=\K[0-9.]+%' || echo "?")
  echo "  fast: ${n}/500 ${rate}"
  sleep 120
done
echo "[resume] fast done; copying final JSON if present"
if [[ -f "${REPO}/benchmarks/results/longmemeval-session-v2-fast.json" ]]; then
  echo "  -> benchmarks/results/longmemeval-session-v2-fast.json"
fi

done_mpnet=$(wc -l < "$MPNET_CKPT" 2>/dev/null || echo 0)
if [[ "$done_mpnet" -ge 500 ]]; then
  echo "[resume] mpnet already complete (${done_mpnet}/500)"
  exit 0
fi

echo "[resume] starting mpnet from checkpoint (${done_mpnet}/500 done)..."
export FLUCTLIGHT_EMBED_URL=http://127.0.0.1:8794
# Kill fast bench if still running
pkill -f 'longmemeval-v2-fast.jsonl' 2>/dev/null || true
sleep 2
nohup "${REPO}/scripts/longmemeval-v2-run.sh" full-mpnet >> /tmp/longmemeval-v2-mpnet-resume.log 2>&1 &
echo "[resume] mpnet pid=$! log=/tmp/longmemeval-v2-mpnet-resume.log"
