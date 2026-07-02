#!/usr/bin/env bash
# Ping owner Telegram when LongMemEval checkpoint reaches 500 (or results file updates).
set -euo pipefail
CHECKPOINT="${CHECKPOINT:-/tmp/longmemeval-checkpoint.jsonl}"
RESULTS="${RESULTS:-/home/ambugo/fluctlightdb/benchmarks/results/longmemeval-$(date +%Y-%m-%d).json}"
OWNER_CHAT="${SB_OWNER_CHAT:-6153332713}"
POLL_SEC="${POLL_SEC:-45}"
TARGET="${TARGET:-500}"

send_tg() {
  local text="$1"
  local token
  token=$(python3 - <<'PY'
import os, sys
sys.path.insert(0, "/opt/ambugo/serverbrain-v2")
try:
    import serverbrain_common as sb
    print(sb.telegram_token())
except Exception:
    print(os.environ.get("TELEGRAM_BOT_TOKEN", ""))
PY
)
  [ -n "$token" ] || { echo "no telegram token"; return 1; }
  curl -s --max-time 30 -X POST "https://api.telegram.org/bot${token}/sendMessage" \
    --data-urlencode "chat_id=${OWNER_CHAT}" \
    --data-urlencode "text=${text}" >/dev/null
}

echo "Watching LongMemEval (target=${TARGET}, poll=${POLL_SEC}s)…"
while true; do
  if [ -f "$CHECKPOINT" ]; then
    n=$(wc -l < "$CHECKPOINT" | tr -d ' ')
    if [ "${n:-0}" -ge "$TARGET" ]; then
      summary=$(python3 - <<PY
import json
from pathlib import Path
p = Path("$RESULTS")
if p.is_file():
    d = json.loads(p.read_text())
    s = d.get("summary") or d
else:
  rows = [json.loads(l) for l in Path("$CHECKPOINT").read_text().splitlines() if l.strip()]
  hits = sum(1 for r in rows if r.get("hit"))
  s = {"questions": len(rows), "answer_in_recall_at_k": hits/len(rows) if rows else 0, "hits": f"{hits}/{len(rows)}"}
print(f"LongMemEval-S done: {s.get('hits')} @ recall@{s.get('top_k', 8)} = {float(s.get('answer_in_recall_at_k', 0))*100:.1f}%")
print(f"Mode: {s.get('mode','index')} | wall: {s.get('wall_s','?')}s | sec/Q: {s.get('sec_per_question','?')}")
PY
)
      send_tg "✅ FluctlightDB LongMemEval complete

${summary}

Results: ${RESULTS}"
      echo "Notified. Done."
      exit 0
    fi
    echo "$(date -Is) progress ${n}/${TARGET}"
  fi
  sleep "$POLL_SEC"
done
