#!/usr/bin/env bash
# Build and install the Python native extension (library call, like sqlite3).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/crates/fluctlight-py"
if ! command -v maturin >/dev/null 2>&1; then
  python3 -m pip install --user maturin
  export PATH="$HOME/.local/bin:$PATH"
fi
maturin build --release
WHEEL="$(ls -t "$ROOT/target/wheels/"fluctlightdb_native*.whl | head -1)"
python3 -m pip install --user --force-reinstall "$WHEEL"
echo "Installed fluctlightdb_native from $WHEEL"
python3 -c "import fluctlightdb_native as n; print('fluctlightdb_native', n.__version__)"
