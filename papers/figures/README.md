# Paper figures (downloadable)

Vector PDF + PNG diagrams for the FluctlightDB research paper.  
Frozen metrics source: [`benchmarks/results/paper-2026-07-03.json`](../../benchmarks/results/paper-2026-07-03.json).

## Download

| Figure | PDF | PNG | Description |
|--------|-----|-----|-------------|
| **1** | [01-brain-architecture.pdf](01-brain-architecture.pdf) | [01-brain-architecture.png](01-brain-architecture.png) | Brain directory, engram record, `experience()` / `activate()` paths |
| **2** | [02-benchmark-summary.pdf](02-benchmark-summary.pdf) | [02-benchmark-summary.png](02-benchmark-summary.png) | Headline results: LoCoMo, LongMemEval-S, BEIR, FAMB |
| **3** | [03-longmemeval-by-type.pdf](03-longmemeval-by-type.pdf) | [03-longmemeval-by-type.png](03-longmemeval-by-type.png) | LongMemEval-S session@8 breakdown by question type |

## Regenerate

```bash
pip install matplotlib
python3 papers/figures/generate_all.py
```

Then rebuild the PDF:

```bash
cd papers/arxiv-v1 && bash build.sh
```

## Used in

- LaTeX: `papers/arxiv-v1/main.tex` (Figures 1–2)
- Public viewer: `papers/public/assets/`
- arXiv checklist: `papers/arxiv-v1/ARXIV_SUBMISSION.md`

## License

MIT — same as the FluctlightDB repository.
