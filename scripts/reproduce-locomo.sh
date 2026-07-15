#!/usr/bin/env bash
# Reproduce LoCoMo HONEST raw evidence recall (no neighbor expansion) and compare
# to the frozen cert. The historical "99.0%" used a ±3 neighbor-expansion scoring
# trick and is deprecated — this reproduces the honest number the engine earns.
# Usage (from repo root):
#   ./scripts/reproduce-locomo.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

LOCOMO_DIR="${LOCOMO_DATA_DIR:-/tmp/locomo}"
LOCOMO_JSON="${LOCOMO_DATA:-$LOCOMO_DIR/locomo10.json}"
# Self-contained honest recipe: token-population late interaction (MaxSim) ⊕ BM25,
# builds its own MiniLM token cache via onnxruntime. No native maxsim binding needed.
FROZEN="${FROZEN:-benchmarks/results/locomo-lateinteraction-2026-07-13.json}"
OUT="${OUT:-benchmarks/results/locomo-reproduce-$(date +%Y-%m-%d).json}"
VENV="${VENV:-$ROOT/.reproduce-venv}"

echo "==> LoCoMo reproduce — HONEST raw recall@k (no expansion)"
echo "    dataset:  $LOCOMO_JSON"
echo "    frozen:   $FROZEN"
echo "    output:   $OUT"

mkdir -p "$LOCOMO_DIR"
if [[ ! -f "$LOCOMO_JSON" ]]; then
  echo "==> Downloading LoCoMo locomo10.json ..."
  curl -fsSL -o "$LOCOMO_JSON" \
    "https://raw.githubusercontent.com/snap-research/locomo/main/data/locomo10.json"
fi

if [[ ! -d "$VENV" ]]; then
  python3 -m venv "$VENV"
fi
# shellcheck disable=SC1091
source "$VENV/bin/activate"
pip install -q --upgrade pip
pip install -q -r benchmarks/requirements-reproduce.txt onnxruntime tokenizers numpy

export LOCOMO_DATA="$LOCOMO_JSON"
export PYTHONPATH="$ROOT/sdks/python${PYTHONPATH:+:$PYTHONPATH}"

echo "==> Running honest late-interaction benchmark (builds token cache on first run) ..."
python benchmarks/locomo_lateinteraction.py --data "$LOCOMO_JSON" --json-out "$OUT"

echo "==> Comparing raw recall@150 to frozen cert ..."
python - "$OUT" "$FROZEN" <<'PY'
import json, sys
got, frozen = map(json.load, map(open, sys.argv[1:3]))
g = got.get("recall_at_k", {}).get("150")
f = frozen.get("recall_at_k", {}).get("150")
if g is None or f is None:
    print(f"could not read recall_at_k[150]: got {g!r} frozen {f!r}"); sys.exit(1)
if abs(float(g) - float(f)) > 0.5:
    print(f"MISMATCH raw recall@150: got {g} frozen {f}"); sys.exit(1)
print(f"PASS — honest raw recall@150 matches frozen cert ({g} ~ {f}).")
PY

echo "==> Done. Result: $OUT"
echo "    Note: the native-engine invented-stack numbers (96.8% MiniLM / 97.0% mpnet)"
echo "    are higher and reproduce via benchmarks/locomo_engine_maxsim.py after a source build."
