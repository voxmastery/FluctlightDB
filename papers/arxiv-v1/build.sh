#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
for cmd in pdflatex bibtex; do
  command -v "$cmd" >/dev/null || { echo "Install texlive-latex-base and bibtex"; exit 1; }
done
python3 ../figures/generate_all.py
# arXiv layout: figure PDFs live in figures/ beside main.tex (this dir also holds
# architecture source assets; copy the four paper figures from papers/figures).
FIG_SRC=../figures
for f in 01-brain-architecture.pdf 02-benchmark-summary.pdf \
         03-longmemeval-by-type.pdf 04-longmemeval-e2e-by-type.pdf; do
  cp -f "$FIG_SRC/$f" figures/
done
pdflatex -interaction=nonstopmode main.tex
bibtex main || true
pdflatex -interaction=nonstopmode main.tex
pdflatex -interaction=nonstopmode main.tex
echo "Built: $(pwd)/main.pdf"

# Source bundle for arXiv (flat: main.tex + figures/*.pdf at zip root paths).
python3 <<'PY'
import zipfile
from pathlib import Path

root = Path(".")
fig_dir = root / "figures"
needed = [
    "01-brain-architecture.pdf",
    "02-benchmark-summary.pdf",
    "03-longmemeval-by-type.pdf",
    "04-longmemeval-e2e-by-type.pdf",
]
out = root / "fluctlightdb-arxiv-source.zip"
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    for name in ("main.tex", "references.bib", "main.bbl"):
        z.write(root / name, name)
    for pdf in needed:
        path = fig_dir / pdf
        if not path.is_file():
            raise SystemExit(f"missing figure: {path}")
        z.write(path, f"figures/{pdf}")
print(f"Wrote {out.resolve()} ({out.stat().st_size} bytes)")
PY
