# FluctlightDB Paper Series

Publication order (planned):

| # | Venue | Title (working) | Status |
|---|-------|-----------------|--------|
| 1 | **arXiv cs.DB** | FluctlightDB: A Brain-Native Memory Engine for AI Agents | `arxiv-v1/` draft started |
| 2 | **CIDR 2027** (target) | Short visionary / systems paper | outline after arXiv feedback |
| 3 | **VLDB/SIGMOD** (target) | Full evaluation + scale | after benchmark freeze |

## Paper 1 — arXiv cs.DB

Directory: `papers/arxiv-v1/`

```bash
cd papers/arxiv-v1
# requires latex: pdflatex main.tex && bibtex main && pdflatex main.tex
./build.sh
```

See `docs/BENCHMARKS.md` for eval protocol and `docs/RESEARCH.md` for submission checklist.

## Results archive

After each benchmark run, copy JSON/summary to `benchmarks/results/` with date stamp.
