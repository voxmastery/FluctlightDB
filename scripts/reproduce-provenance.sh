#!/usr/bin/env bash
# Reproduce graded provenance-conflict benchmark (50 cases, agent lane).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PYTHONPATH="${ROOT}/sdks/python${PYTHONPATH:+:$PYTHONPATH}"
OUT="${1:-benchmarks/results/provenance-conflict-$(date +%Y-%m-%d).json}"
python3 benchmarks/provenance_conflict_bench.py --json-out "$OUT"
echo "Wrote $OUT"
