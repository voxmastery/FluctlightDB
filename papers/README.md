# FluctlightDB Paper Series

LaTeX preprint and reproducible benchmarks live in this repository.

| Artifact | Location |
|----------|----------|
| **Manuscript (LaTeX)** | `papers/arxiv-v1/` |
| **Figures (download)** | [`papers/figures/`](figures/) — PDF + PNG |
| **arXiv checklist** | `papers/arxiv-v1/ARXIV_SUBMISSION.md` |
| **Static viewer** | `papers/public/` (HF Space source) |
| **Frozen metrics** | `benchmarks/results/paper-2026-07-07.json` |
| **Cite** | [CITATION.cff](../CITATION.cff) · [Zenodo DOI](https://doi.org/10.5281/zenodo.20949890) |

## Build PDF

```bash
cd papers/arxiv-v1
bash build.sh
# generates papers/figures/*.pdf + *.png, then main.pdf
```

See [`papers/figures/README.md`](figures/README.md) to download diagrams without building the PDF.

## Sync public viewer + Hugging Face

```bash
bash scripts/sync-paper-public.sh
bash scripts/publish-paper-huggingface.sh   # after hf auth login
```

Eval protocol: `docs/BENCHMARKS.md`.
