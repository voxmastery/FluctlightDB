# arXiv submission checklist — FluctlightDB (July 2026)

Use this after `paper-2026-07-09.json` metrics are frozen and `main.tex` builds clean.

## 1. Pre-flight (metrics & reproducibility)

- [x] `benchmarks/results/paper-2026-07-09.json` — merged frozen numbers (retrieval + E2E)
- [x] `benchmarks/results/e2e-cert-paper-v2-2026-07-07.json` — LongMemEval E2E 500 (paper profile)
- [x] `benchmarks/results/longmemeval-colab-v2-full-2026-07-04.json` — LongMemEval unified 500 retrieval
- [x] `benchmarks/results/2025-06-22.json` — LoCoMo + BEIR + FAMB
- [x] Colab notebook runs end-to-end: `benchmarks/longmemeval_colab_v2.ipynb`
- [x] `papers/figures/` — all diagrams (PDF + PNG); Figure 1 Playwright audit passes

```bash
cd papers/figures && python3 render_fig1_playwright.py && python3 audit_fig1_playwright.py
python3 papers/figures/generate_all.py
cd papers/arxiv-v1 && bash build.sh
bash scripts/sync-paper-public.sh
```

## 2. arXiv account & metadata

| Field | Value |
|-------|--------|
| **Primary category** | `cs.DB` (Databases) |
| **Secondary** | `cs.AI`, `cs.IR` (optional) |
| **Title** | FluctlightDB: A Memory Model of Data for AI Agents |
| **Authors** | Ganesh S (ORCID 0009-0006-7758-4114) |
| **Abstract** | Copy from `main.tex` abstract block |
| **Comments** | 12 pages, 5 tables, 4 figures; code and frozen metrics at GitHub + Zenodo |
| **License** | arXiv non-exclusive (repo MIT) |

Register: https://arxiv.org/user/register — use the same email as the paper (`voxmastery@gmail.com`).

## 3. Files to upload

| Upload | Path |
|--------|------|
| PDF | `papers/arxiv-v1/main.pdf` |
| Source (optional but recommended) | Zip `main.tex`, `references.bib`, `main.bbl`, `../figures/*.pdf` |

```bash
python3 papers/figures/generate_all.py
cd papers/arxiv-v1
zip -r fluctlightdb-arxiv-source.zip main.tex references.bib main.bbl ../figures/*.pdf
```

## 4. Headline numbers (for abstract & tweet thread)

| Benchmark | Metric | Score |
|-----------|--------|------:|
| LoCoMo | Evidence recall | **99.0%** (1970/1982, k=150) |
| LongMemEval-S | session_recall@8 (v4 retrieval) | **97.6%** (488/500 unified v4) |
| LongMemEval-S E2E | overall accuracy | **97.4%** (487/500, paper profile) |
| LongMemEval-S E2E | task-averaged accuracy | **98.2%** |
| LongMemEval-S E2E | session@8 (same pipeline) | **100%** (500/500, Muon) |
| LongMemEval preference | session_recall@8 | **96.7%** (29/30, v4 mpnet) |
| BEIR SciFact | nDCG@10 | **0.645** (ties Chroma + MiniLM) |
| FAMB | Macro | **97–98%** |

LongMemEval retrieval by type: knowledge-update 100%, multi-session 98.5%, user 97.1%, assistant 98.2%, temporal 95.5%, preference 96.7%.

LongMemEval E2E by type: user/assistant/preference 100%, temporal 99.3%, knowledge-update 97.4%, multi-session 92.5%.

## 5. After arXiv accepts

1. Note the arXiv ID (e.g. `arXiv:2607.xxxxx`).
2. Update `CITATION.cff` `preferred-citation` with arXiv ID.
3. Update `README.md`, `hub/paper/README.md`, `papers/public/index.html` — replace "arXiv pending".
4. New Zenodo version (optional): attach `main.pdf` + `paper-2026-07-09.json` + E2E JSON.
5. Sync Hugging Face:

```bash
bash scripts/sync-paper-public.sh
bash scripts/publish-paper-huggingface.sh   # after hf auth login
```

## 6. Impact positioning (honest claims)

**Say:**
- Third data model for agent memory; embedded engine with `experience()` / `activate()`.
- 99.0% LoCoMo evidence recall (official retrieval metric).
- 97.6% LongMemEval-S session recall@8 (unified v4 488/500) — competitive with published hybrid systems.
- **97.4% LongMemEval-S end-to-end QA** (487/500, official Wu et al. judge; paper profile).
- 96.7% on preference slice (29/30) with v4 pref-facts harness.
- Open harnesses + frozen JSON; reproducible via Colab notebook and `e2e_certify.sh`.

**Do not say:**
- "SOTA on LongMemEval" (gbrain 97.6% @5 uses different K and embedder).
- "98%+ overall E2E" without noting overall is 97.4% (task-averaged is 98.2%).
- "90%+ on all question types" without noting E2E multi-session is 92.5%.

## 7. Suggested arXiv abstract (matches main.tex)

Use the `\begin{abstract}...\end{abstract}` block from `main.tex` verbatim.

## 8. Related submissions

- Zenodo DOI already live: https://doi.org/10.5281/zenodo.20949890
- HF paper card: https://huggingface.co/Voxiesz/fluctlightdb-paper
- GitHub: https://github.com/voxmastery/FluctlightDB
