#!/usr/bin/env bash
# Merge E2E JSON into paper freeze, patch main.tex Table 2, rebuild PDF + public site.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

E2E_JSON="${1:-benchmarks/results/longmemeval-e2e-v4-mpnet.json}"
FULL_JSON="${2:-benchmarks/results/longmemeval-colab-v2-full-2026-07-04.json}"
PAPER_JSON="${3:-benchmarks/results/paper-2026-07-04.json}"
TEX="$ROOT/papers/arxiv-v1/main.tex"

if [[ ! -f "$E2E_JSON" ]]; then
  echo "Missing E2E JSON: $E2E_JSON" >&2
  exit 1
fi

python3 "$ROOT/scripts/freeze-paper-v2.py" \
  --full "$FULL_JSON" \
  --e2e "$E2E_JSON" \
  --base "$PAPER_JSON" \
  --out "$PAPER_JSON"

python3 - <<PY
import json
import re
from pathlib import Path

e2e = json.loads(Path("$E2E_JSON").read_text())
summary = e2e.get("summary") or e2e
acc = float(summary.get("overall_accuracy") or 0)
pct = f"{acc * 100:.1f}\\%"
tex = Path("$TEX")
text = tex.read_text()
new = (
    f"FluctlightDB (v4) & \\\\textbf{{97.6\\\\%}} & \\\\textbf{{{pct}}} "
    f"& Gemini 2.5 Flash reader + judge (Wu et al. judge prompts) \\\\"
)
old = (
    r"FluctlightDB \(v4\) & \\textbf\{97\.6\\%\} & "
    r"(?:\(Colab run\)|\(pending\)|\\textbf\{[0-9.]+\%\}) & [^\\\\]+\\\\"
)
if not re.search(old, text):
    raise SystemExit("Could not find FluctlightDB E2E table row in main.tex")
text = re.sub(old, new, text, count=1)
tex.write_text(text)
print(f"Patched main.tex E2E accuracy -> {pct}")
PY

python3 "$ROOT/papers/figures/generate_all.py"
bash "$ROOT/papers/arxiv-v1/build.sh"
bash "$ROOT/scripts/sync-paper-public.sh"
echo "Done: $PAPER_JSON + papers/arxiv-v1/main.pdf"
