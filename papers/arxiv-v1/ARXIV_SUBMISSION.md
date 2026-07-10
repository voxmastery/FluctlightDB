# arXiv submission checklist — FluctlightDB (July 2026)

Submit **`papers/arxiv-v1/main.pdf`** as the full expanded preprint (not a short version).

## 1. Pre-flight

```bash
cd papers/arxiv-v1 && bash build.sh   # or: pdflatex + bibtex ×2
python3 papers/figures/generate_all.py   # if figures changed
bash scripts/sync-paper-public.sh
```

Frozen artifacts (must match `main.tex`):

- `benchmarks/results/paper-2026-07-09.json`
- `benchmarks/results/locomo-chorus-fabric-2026-07-09.json`
- `benchmarks/results/beir-prism-fabric-2026-07-09.json`
- `benchmarks/results/famb-*-fabric-2026-07-09.json`
- `benchmarks/results/longmemeval-colab-v2-full-2026-07-04.json`
- `benchmarks/results/e2e-cert-paper-v2-2026-07-07.json`

## 2. arXiv metadata

| Field | Value |
|-------|--------|
| **Primary category** | `cs.DB` |
| **Secondary** | `cs.AI`, `cs.IR` (optional) |
| **Title** | FluctlightDB: A Memory Model of Data for AI Agents |
| **Authors** | Ganesh S (ORCID 0009-0006-7758-4114) |
| **Abstract** | Copy verbatim from `main.tex` `\begin{abstract}...\end{abstract}` |
| **Comments** | ~14 pages, 6 tables, 4 figures; code + frozen metrics on GitHub + Zenodo |
| **License** | arXiv non-exclusive (repo MIT) |

Register: https://arxiv.org/user/register

## 3. Upload

| File | Path |
|------|------|
| **PDF (required)** | `papers/arxiv-v1/main.pdf` |
| Source zip (recommended) | `main.tex`, `references.bib`, `main.bbl`, `../figures/*.pdf` |

```bash
cd papers/arxiv-v1 && bash build.sh
```

Produces `main.pdf` and `fluctlightdb-arxiv-source.zip` (`main.tex`, `references.bib`, `main.bbl`, `figures/*.pdf` with paths matching `main.tex`).

## 4. Headline numbers (July 2026 freeze)

| Benchmark | Metric | Score |
|-----------|--------|------:|
| LoCoMo | Evidence recall (Fabric on) | **99.0%** (1970/1982, k=150) |
| LongMemEval-S | session_recall@8 (hybrid index, no Fabric) | **97.6%** (488/500) |
| LongMemEval-S E2E | overall accuracy | **97.4%** (487/500) |
| BEIR SciFact | nDCG@10 / R@10 (Fabric on) | **0.646 / 0.792** vs Chroma 0.645 / 0.783 |
| FAMB | Macro (internal regression) | **100%** |

## 5. Honest claims

**Say:** embedded engine (SQLite-style), candidate third data model, Fabric-on for CHORUS benchmarks, hybrid index for LongMemEval, chaos-tested durability.

**Do not say:** strict LongMemEval SOTA; production case study; BEIR dominates Chroma on every metric.

## 6. After acceptance

1. Add arXiv ID to `CITATION.cff`, README, `papers/public/index.html`.
2. Optional Zenodo version with `main.pdf` + freeze JSON.
3. `bash scripts/sync-paper-public.sh && bash scripts/publish-paper-huggingface.sh`
