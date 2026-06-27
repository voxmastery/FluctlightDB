# FluctlightDB Paper Series

LaTeX preprint and reproducible benchmarks live in this repository.

| Artifact | Location |
|----------|----------|
| **Manuscript (LaTeX)** | `papers/arxiv-v1/` |
| **Static viewer** | `papers/public/` (HF Space source) |
| **Frozen metrics** | `benchmarks/results/` |
| **Cite** | [CITATION.cff](../CITATION.cff) · [Zenodo DOI](https://doi.org/10.5281/zenodo.20949890) |

## Build PDF

```bash
cd papers/arxiv-v1
./build.sh   # requires pdflatex + bibtex
```

## Sync public viewer + Hugging Face

```bash
bash scripts/sync-paper-public.sh
bash scripts/publish-paper-huggingface.sh   # after hf auth login
```

Eval protocol: `docs/BENCHMARKS.md`.
