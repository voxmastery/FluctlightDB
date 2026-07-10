#!/usr/bin/env bash
# Reproduce graded provenance-conflict benchmark (50 cases, agent lane).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export PYTHONPATH="${ROOT}/sdks/python${PYTHONPATH:+:$PYTHONPATH}"
OUT="${1:-benchmarks/results/provenance-conflict-$(date +%Y-%m-%d).json}"
SHARED="${2:-benchmarks/results/provenance-conflict-shared-$(date +%Y-%m-%d).json}"
python3 benchmarks/provenance_conflict_bench.py --json-out "$OUT"
python3 benchmarks/provenance_conflict_bench.py --shared-brain --json-out "$SHARED"
echo "Wrote $OUT (isolated) and $SHARED (shared-brain)"
