#!/usr/bin/env bash
# Smoke BEAM retrieval eval (single 100K chat). Does not require LLM API keys.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VENV="${VENV:-$ROOT/.beam-venv}"
OUT="${OUT:-benchmarks/results/beam-smoke-$(date +%Y-%m-%d).json}"

if [[ ! -d "$VENV" ]]; then
  python3 -m venv "$VENV"
fi
# shellcheck disable=SC1091
source "$VENV/bin/activate"
pip install -q --upgrade pip
pip install -q -r benchmarks/requirements-reproduce.txt

NATIVE_VER="$(python3 -c "import importlib.util; p='crates/fluctlight-py/pyproject.toml'; import re; print(re.search(r'version\\s*=\\s*\"([^\"]+)\"', open(p).read()).group(1))")"
pip install -q "fluctlightdb[native]==${NATIVE_VER}" || {
  pip install -q maturin
  maturin build --release -o "$ROOT/dist-beam" --manifest-path crates/fluctlight-py/Cargo.toml
  pip install -q -e sdks/python "$ROOT"/dist-beam/*.whl
}

export PYTHONPATH="$ROOT/sdks/python${PYTHONPATH:+:$PYTHONPATH}"
python benchmarks/beam_eval.py --smoke --json-out "$OUT"
echo "==> Done: $OUT"
