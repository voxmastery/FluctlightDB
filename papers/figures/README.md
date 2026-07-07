# Paper figures (downloadable)

Vector PDF + PNG diagrams for the FluctlightDB research paper.  
Frozen metrics source: [`benchmarks/results/paper-2026-07-07.json`](../../benchmarks/results/paper-2026-07-07.json).

## Download

| Figure | PDF | PNG | Description |
|--------|-----|-----|-------------|
| **1** (full) | [01-brain-architecture.pdf](01-brain-architecture.pdf) | [01-brain-architecture.png](01-brain-architecture.png) | arXiv Figure 1: activation graph + technical panels |
| **1** (hero) | [01-brain-hero.pdf](01-brain-hero.pdf) | [01-brain-hero.png](01-brain-hero.png) | README / landing — cue-driven activation graph only |
| **2** | [02-benchmark-summary.pdf](02-benchmark-summary.pdf) | [02-benchmark-summary.png](02-benchmark-summary.png) | Headline results: LoCoMo, LongMemEval-S, BEIR, FAMB |
| **3** | [03-longmemeval-by-type.pdf](03-longmemeval-by-type.pdf) | [03-longmemeval-by-type.png](03-longmemeval-by-type.png) | LongMemEval-S session@8 breakdown by question type |

Design spec: [`docs/superpowers/specs/2026-07-03-brain-figure-redesign-design.md`](../../docs/superpowers/specs/2026-07-03-brain-figure-redesign-design.md)

## Regenerate

```bash
pip install matplotlib networkx playwright
python3 -m playwright install chromium   # one-time
python3 papers/figures/generate_all.py
```

Figure 1 uses **Playwright** (`fig1_template.html`) for clean layout. Bar charts (Fig 2–3) use matplotlib.

Then rebuild the PDF:

```bash
cd papers/arxiv-v1 && bash build.sh
```

## Used in

- LaTeX: `papers/arxiv-v1/main.tex` (Figures 1–3)
- Public viewer: `papers/public/assets/`
- arXiv checklist: `papers/arxiv-v1/ARXIV_SUBMISSION.md`

## License

MIT — same as the FluctlightDB repository.
