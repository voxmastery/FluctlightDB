# FluctlightDB research paper — draft status

**LaTeX source:** `papers/arxiv-v1/main.tex`  
**Frozen metrics:** `benchmarks/results/2025-06-22.json`

## What’s done

| Benchmark | Metric | Result | LLM required? |
|-----------|--------|--------|---------------|
| BEIR SciFact | nDCG@10 / R@10 / R@100 | 0.645 / 0.783 / 0.925 (matches Chroma) | No |
| FAMB (index) | Macro | 98% | No |
| FAMB (agent) | Macro | 97% | No |
| **LoCoMo full** | **Mean evidence recall** | **98.1%** (1925/1982) | No (retrieval) |
| LoCoMo | Has-answer-in-context | 37.9% | No (verbatim proxy) |
| LoCoMo LLM QA | Category F1 (50 Q pilot) | 23.5% (Cloudflare) | Yes |
| LongMemEval-S | Answer-in-recall@8 | **Deferred** (CPU; pilot 70% n=20) | No |

## Paper viewer

Draft LaTeX: `papers/arxiv-v1/main.tex` in the repo. Private HTML viewer on VPS (not linked from public README).

- **Draft** tab — markdown mirror of the paper
- **Format & models** tab — arXiv checklist + which LLM to use for each writing task
- **Downloads** — `main.tex`, `references.bib`, `results.json`, `draft.md`

Deploy/update: `papers/site/install-search-site.sh`

## Build PDF

```bash
cd papers/arxiv-v1
pdflatex main.tex
bibtex main
pdflatex main.tex
pdflatex main.tex
```

## Before arXiv submission

1. Paste LongMemEval full numbers into `main.tex` Table 4 when `/tmp/longmemeval_full.json` completes.
2. Optional: add Mem0/Zep LoCoMo retrieval comparison row (cite their papers; run same metric).
3. Optional: LLM QA column when API quota resets (Groq/Cloudflare with throttled harness).
4. Add author ORCID; primary arXiv category **cs.DB** (cross-list cs.AI, cs.IR).

## Key narrative

- **Problem:** Vector DBs ≠ agent memory (no provenance, consolidation, cue activation).
- **Method:** Brain-native write/read path + dual API (`connect` vs `connect_index`).
- **Headline result:** LoCoMo **98.1% evidence recall** on full 10-conversation set with hybrid retrieval.
- **Honest limit:** End-to-end QA depends on reader LLM; retrieval layer is what we fully measured.

## Commands to reproduce

```bash
# LoCoMo retrieval (98.1%)
LOCOMO_THREADS=2 ./benchmarks/run_locomo_eval.sh \
  --mode index --rag-mode all --top-k 150 --isolated \
  --json-out /tmp/locomo_final.json

# LongMemEval retrieval (no LLM)
LOCOMO_THREADS=2 ./benchmarks/run_longmemeval.sh

# BEIR
BEIR_DATA=/tmp/beir BEIR_DS=scifact MODE=index python benchmarks/beir_bench.py
```
