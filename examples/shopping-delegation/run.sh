#!/bin/sh
# One-command repro from a checkout that already has fluctlightdb and
# fluctlightdb_native importable (see README). Runs the demo, then the benchmark.
set -eu
cd "$(dirname "$0")"
REPO=$(CDPATH= cd ../.. && pwd)
export PYTHONPATH="$REPO/sdks/python${PYTHONPATH:+:$PYTHONPATH}"
python3 -c "import fluctlightdb, fluctlightdb_native" || {
  echo "Need the in-repo SDK and the native extension on PYTHONPATH." >&2
  echo "  pip install -e sdks/python" >&2
  echo "  maturin build --release -o /tmp/fluctlight-wheels --manifest-path crates/fluctlight-py/Cargo.toml" >&2
  echo "  pip install /tmp/fluctlight-wheels/fluctlightdb_native-*.whl" >&2
  exit 1
}
python3 -m shopping_delegation selftest
python3 -m shopping_delegation demo
python3 -m shopping_delegation bench
