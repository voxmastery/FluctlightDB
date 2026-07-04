#!/usr/bin/env bash
# Print E2E 500 progress from checkpoint + log tail.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CKPT="$ROOT/benchmarks/results/longmemeval-e2e-v4-mpnet.checkpoint.jsonl"
LOG="${1:-/tmp/longmemeval-e2e-500.log}"

python3 - <<PY
import json
from pathlib import Path

p = Path("$CKPT")
if not p.is_file():
    print("No checkpoint yet:", p)
    raise SystemExit(0)
rows = [json.loads(l) for l in p.read_text().splitlines() if l.strip()]
judged = [r for r in rows if r.get("autoeval_label") is not None]
acc = sum(1 for r in judged if r.get("autoeval_label")) / len(judged) if judged else 0
retr = sum(1 for r in rows if r.get("session_recall_hit")) / len(rows) if rows else 0
avg_sec = sum(r.get("sec", 0) for r in rows) / len(rows) if rows else 0
left = max(0, 500 - len(rows))
eta_h = (left * avg_sec / max(1, int("${LONGMEMEVAL_E2E_WORKERS:-4}"))) / 3600
print(f"E2E progress: {len(rows)}/500  session@8={retr:.1%}  accuracy={acc:.1%}  avg_sec={avg_sec:.0f}  eta~{eta_h:.1f}h")
PY

if [[ -f "$LOG" ]]; then
  echo "--- log tail ---"
  tail -5 "$LOG"
fi

pgrep -af "longmemeval_e2e" >/dev/null && echo "status: running" || echo "status: stopped"
