#!/usr/bin/env bash
# Reproduce LoCoMo CHORUS evidence recall and compare to frozen cert.
# Usage (from repo root):
#   ./scripts/reproduce-locomo.sh
#   REPRODUCE_FROM_SOURCE=1 ./scripts/reproduce-locomo.sh   # build native from source
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

LOCOMO_DIR="${LOCOMO_DATA_DIR:-/tmp/locomo}"
LOCOMO_JSON="${LOCOMO_DATA:-$LOCOMO_DIR/locomo10.json}"
FROZEN="${FROZEN:-benchmarks/results/locomo-chorus-2026-07-08.json}"
OUT="${OUT:-benchmarks/results/locomo-reproduce-$(date +%Y-%m-%d).json}"
VENV="${VENV:-$ROOT/.reproduce-venv}"
TOP_K="${TOP_K:-150}"

echo "==> LoCoMo reproduce (CHORUS, k=$TOP_K)"
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

if [[ "${REPRODUCE_FROM_SOURCE:-0}" == "1" ]]; then
  echo "==> Installing from source (maturin + editable SDK)"
  pip install -q -r benchmarks/requirements-reproduce.txt maturin
  maturin build --release -o "$ROOT/dist-reproduce" --manifest-path crates/fluctlight-py/Cargo.toml
  pip install -q -e sdks/python
  pip install -q "$ROOT"/dist-reproduce/*.whl
else
  NATIVE_VER="$(python3 -c "import tomllib; print(tomllib.load(open('crates/fluctlight-py/pyproject.toml','rb'))['project']['version'])")"
  echo "==> Installing pinned deps (fluctlightdb[native]==${NATIVE_VER})"
  pip install -q -r benchmarks/requirements-reproduce.txt "fluctlightdb[native]==${NATIVE_VER}" || {
    echo "WARN: PyPI native wheel not yet published for ${NATIVE_VER}."
    echo "      Falling back to source build (set REPRODUCE_FROM_SOURCE=1 to skip this message)."
    pip install -q maturin -r benchmarks/requirements-reproduce.txt
    maturin build --release -o "$ROOT/dist-reproduce" --manifest-path crates/fluctlight-py/Cargo.toml
    pip install -q -e sdks/python
    pip install -q "$ROOT"/dist-reproduce/*.whl
  }
fi

python -c "import fluctlightdb_native; import fluctlightdb; print('native import ok')"

export LOCOMO_DATA="$LOCOMO_JSON"
export PYTHONPATH="$ROOT/sdks/python${PYTHONPATH:+:$PYTHONPATH}"

echo "==> Running locomo_eval.py ..."
python benchmarks/locomo_eval.py \
  --mode chorus \
  --top-k "$TOP_K" \
  --json-out "$OUT"

echo "==> Comparing to frozen cert ..."
python - "$OUT" "$FROZEN" <<'PY'
import json, sys
got, frozen = map(json.load, map(open, sys.argv[1:3]))
for key in ("evidence_hits", "mean_evidence_recall"):
    g, f = got.get(key), frozen.get(key)
    if g != f:
        print(f"MISMATCH {key}: got {g!r} frozen {f!r}")
        sys.exit(1)
    print(f"match {key}: {g}")
print("PASS — reproduction matches frozen LoCoMo cert.")
PY

echo "==> Done. Result: $OUT"
