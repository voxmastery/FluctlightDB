#!/usr/bin/env bash
# World launch — sync artifacts, build PDF if possible, publish HF, print checklist.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "╔══════════════════════════════════════════════════════════╗"
echo "║  FluctlightDB paper launch                               ║"
echo "╚══════════════════════════════════════════════════════════╝"
echo ""

bash scripts/sync-paper-public.sh

if command -v pdflatex >/dev/null 2>&1; then
  echo "==> Building PDF"
  (cd papers/arxiv-v1 && ./build.sh)
  cp papers/arxiv-v1/main.pdf papers/submit/fluctlightdb-preprint-v1.pdf 2>/dev/null || true
  echo "    PDF: papers/arxiv-v1/main.pdf"
else
  echo "==> PDF: install texlive-latex-base, then: cd papers/arxiv-v1 && ./build.sh"
fi

if hf auth whoami >/dev/null 2>&1; then
  echo ""
  echo "==> Hugging Face"
  bash scripts/publish-paper-huggingface.sh || echo "    HF upload failed — check org permissions"
else
  echo ""
  echo "==> HF: run hf auth login"
fi

echo ""
echo "==> YOUR TURN (highest impact order)"
echo "  1. Zenodo DOI     → zenodo.org + GitHub release tag paper-v1.0"
echo "  2. IEEE Access    → papers/submit/ieee-access/checklist.md"
echo "  3. HN + LinkedIn  → papers/outreach/ (add Zenodo DOI to posts)"
echo "  4. Awesome PRs    → papers/outreach/awesome-list-pr.md"
echo ""
echo "Full playbook: docs/WORLD_LAUNCH.md"
