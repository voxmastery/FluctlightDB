# arXiv submission checklist — FluctlightDB (July 2026)

Use this after `paper-2026-07-03.json` metrics are frozen and `main.tex` builds clean.

## 1. Pre-flight (metrics & reproducibility)

- [ ] `benchmarks/results/paper-2026-07-03.json` — merged frozen numbers
- [ ] `benchmarks/results/longmemeval-colab-mpnet-2026-07-03.json` — LongMemEval detail
- [ ] `benchmarks/results/2025-06-22.json` — LoCoMo + BEIR + FAMB
- [ ] Colab notebook runs end-to-end: `benchmarks/longmemeval_colab.ipynb`
- [ ] `papers/figures/` — all diagrams (PDF + PNG); run `python3 papers/figures/generate_all.py`

```bash
cd papers/arxiv-v1
pdflatex main.tex && bibtex main && pdflatex main.tex && pdflatex main.tex
```

## 2. arXiv account & metadata

| Field | Value |
|-------|--------|
| **Primary category** | `cs.DB` (Databases) |
| **Secondary** | `cs.AI`, `cs.IR` (optional) |
| **Title** | FluctlightDB: A Memory Model of Data for AI Agents |
| **Authors** | Ganesh S (ORCID 0009-0006-7758-4114) |
| **Abstract** | Copy from `main.tex` abstract block |
| **Comments** | 12 pages, 5 tables, 3 figures; code and frozen metrics at GitHub + Zenodo |
| **License** | arXiv non-exclusive (repo MIT) |

Register: https://arxiv.org/user/register — use the same email as the paper (`voxmastery@gmail.com`).

## 3. Files to upload

| Upload | Path |
|--------|------|
| PDF | `papers/arxiv-v1/main.pdf` |
| Source (optional but recommended) | Zip `main.tex`, `references.bib`, `main.bbl` if built |

```bash
python3 papers/figures/generate_all.py
cd papers/arxiv-v1
zip -r fluctlightdb-arxiv-source.zip main.tex references.bib main.bbl ../figures/*.pdf
```

## 4. Headline numbers (for abstract & tweet thread)

| Benchmark | Metric | Score |
|-----------|--------|------:|
| LoCoMo | Evidence recall | **98.1%** (1925/1982, k=150) |
| LongMemEval-S | session_recall@8 | **96.8%** (484/500) |
| BEIR SciFact | nDCG@10 | **0.645** (ties Chroma + MiniLM) |
| FAMB | Macro | **97–98%** |

LongMemEval by type: knowledge-update 100%, multi-session 98.5%, user 98.6%, assistant 98.2%, temporal 96.2%, **preference 76.7%** (honest limitation).

## 5. After arXiv accepts

1. Note the arXiv ID (e.g. `arXiv:2607.xxxxx`).
2. Update `CITATION.cff` `preferred-citation` with arXiv ID.
3. Update `README.md`, `hub/paper/README.md`, `papers/public/index.html` — replace "arXiv pending".
4. New Zenodo version (optional): attach `main.pdf` + `paper-2026-07-03.json`.
5. Sync Hugging Face:

```bash
bash scripts/sync-paper-public.sh
bash scripts/publish-paper-huggingface.sh   # after hf auth login
```

6. Upload dataset card:

```bash
hf upload Voxiesz/fluctlightdb-benchmarks hub/dataset/results.json results.json
hf upload Voxiesz/fluctlightdb-benchmarks hub/dataset/README.md README.md
```

## 6. Impact positioning (honest claims)

**Say:**
- Third data model for agent memory; embedded engine with `experience()` / `activate()`.
- 98.1% LoCoMo evidence recall (official retrieval metric).
- 96.8% LongMemEval-S session recall@8 — competitive with published hybrid systems.
- Open harnesses + frozen JSON; reproducible via Colab notebook.

**Do not say:**
- "SOTA on LongMemEval" (gbrain 97.6% @5 uses different K and embedder).
- "90%+ on all question types" (preference is 76.7%).
- End-to-end QA numbers unless you run the official LLM-judge pipeline.

## 7. Suggested arXiv abstract (matches main.tex)

Use the `\begin{abstract}...\end{abstract}` block from `main.tex` verbatim after the LongMemEval update.

## 8. Related submissions

- Zenodo DOI already live: https://doi.org/10.5281/zenodo.20949890
- HF paper card: https://huggingface.co/Voxiesz/fluctlightdb-paper
- GitHub: https://github.com/voxmastery/FluctlightDB
