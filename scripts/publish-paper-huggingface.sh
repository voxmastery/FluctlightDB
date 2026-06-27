#!/usr/bin/env bash
# Upload paper card + benchmark dataset + static Space to Hugging Face.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ORG="${HF_ORG:-voxmastery}"
PAPER_REPO="${ORG}/fluctlightdb-paper"
BENCH_REPO="${ORG}/fluctlightdb-benchmarks"
SPACE_REPO="spaces/${ORG}/fluctlightdb-paper-viewer"

bash "$ROOT/scripts/sync-paper-public.sh"

echo "Creating/updating HF repos under ${ORG}…"

hf repo create "$PAPER_REPO" --type model --exist-ok --public
hf repo create "$BENCH_REPO" --type dataset --exist-ok --public
hf repo create "$SPACE_REPO" --type space --space-sdk static --exist-ok --public

hf upload "$PAPER_REPO" "$ROOT/hub/paper/README.md" README.md --commit-message "Update paper card"

mkdir -p "$ROOT/.tmp/hf-bench"
cp "$ROOT/benchmarks/results/2025-06-22.json" "$ROOT/.tmp/hf-bench/results.json"
cp "$ROOT/hub/dataset/README.md" "$ROOT/.tmp/hf-bench/README.md"
hf upload "$BENCH_REPO" "$ROOT/.tmp/hf-bench" . --commit-message "Sync frozen benchmark results"

hf upload "$SPACE_REPO" "$ROOT/papers/public" . --commit-message "Sync public paper viewer"

echo ""
echo "Hugging Face published:"
echo "  Paper card:   https://huggingface.co/${PAPER_REPO}"
echo "  Benchmarks:   https://huggingface.co/datasets/${BENCH_REPO}"
echo "  Space viewer: https://huggingface.co/${SPACE_REPO}"
