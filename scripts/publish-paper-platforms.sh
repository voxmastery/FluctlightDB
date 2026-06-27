#!/usr/bin/env bash
# Publish FluctlightDB paper to non-arXiv platforms.
# arXiv is separate — see docs/PUBLISH_PAPER.md
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "==> Sync public paper site"
bash scripts/sync-paper-public.sh

echo ""
echo "==> GitHub Pages"
echo "    Push to main → workflow .github/workflows/paper-pages.yml deploys automatically."
echo "    URL: https://voxmastery.github.io/FluctlightDB/"
echo "    Enable once: Repo Settings → Pages → Source: GitHub Actions"

if command -v pdflatex >/dev/null 2>&1; then
  echo ""
  echo "==> Building PDF"
  (cd papers/arxiv-v1 && ./build.sh)
  cp papers/arxiv-v1/main.pdf papers/public/files/fluctlightdb-preprint.pdf 2>/dev/null || true
  echo "    PDF: papers/arxiv-v1/main.pdf"
else
  echo ""
  echo "==> PDF skip (install texlive-latex-base for local PDF build)"
fi

if [[ -n "${HF_TOKEN:-}" ]] || hf auth whoami >/dev/null 2>&1; then
  echo ""
  echo "==> Hugging Face"
  bash scripts/publish-paper-huggingface.sh
else
  echo ""
  echo "==> Hugging Face skip — run: hf auth login"
  echo "    Then: HF_TOKEN=... bash scripts/publish-paper-huggingface.sh"
fi

if [[ -n "${GITHUB_TOKEN:-}" ]]; then
  echo ""
  echo "==> GitHub About (homepage → paper)"
  export GITHUB_TOKEN
  HOMEPAGE="https://voxmastery.github.io/FluctlightDB/" \
    bash scripts/update-github-about.sh || true
else
  echo ""
  echo "==> GitHub About skip — set GITHUB_TOKEN to update repo homepage"
fi

echo ""
echo "Done. Next manual steps: docs/PUBLISH_PAPER.md (Dev.to, LinkedIn, awesome lists, Zenodo)"
