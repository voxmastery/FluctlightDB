#!/usr/bin/env bash
# Local check: abi3 wheel installs on current Python (mirrors CI pypi-wheel-smoke).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PY="${PYTHON:-python3}"
echo "==> Building native wheel (abi3) + SDK sdist/wheel with maturin/build"
pip install -q maturin build
rm -rf dist-native dist-sdk
maturin build --release -o dist-native --manifest-path crates/fluctlight-py/Cargo.toml --sdist
(cd sdks/python && "$PY" -m build -o "$ROOT/dist-sdk")

echo "==> Clean venv install test ($("$PY" --version))"
VENV="/tmp/flct-wheel-verify-$$"
"$PY" -m venv "$VENV"
"$VENV/bin/pip" install -q --upgrade pip
"$VENV/bin/pip" install dist-native/*.whl dist-sdk/*.whl
"$VENV/bin/python" -c "import fluctlightdb_native; import fluctlightdb; print('OK:', fluctlightdb_native.__name__)"
rm -rf "$VENV"
echo "PASS — wheel installs and imports on $("$PY" --version)"
