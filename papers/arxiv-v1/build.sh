#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
for cmd in pdflatex bibtex; do
  command -v "$cmd" >/dev/null || { echo "Install texlive-latex-base and bibtex"; exit 1; }
done
python3 ../figures/generate_all.py
pdflatex -interaction=nonstopmode main.tex
bibtex main || true
pdflatex -interaction=nonstopmode main.tex
pdflatex -interaction=nonstopmode main.tex
echo "Built: $(pwd)/main.pdf"
