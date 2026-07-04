#!/usr/bin/env bash
# After Colab E2E: copy JSON path → freeze paper Table 2 + PDF.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="${1:?Usage: post-colab-e2e.sh /path/to/longmemeval_colab_e2e_500.json}"
DEST="$ROOT/benchmarks/results/longmemeval-e2e-v4-mpnet.json"
cp "$SRC" "$DEST"
echo "Copied → $DEST"
bash "$ROOT/scripts/finalize-paper-v2-e2e.sh" "$DEST"
