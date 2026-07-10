---
license: mit
task_categories:
  - text-retrieval
  - question-answering
language:
  - en
tags:
  - agent-memory
  - locomo
  - longmemeval
  - beir
  - benchmarks
pretty_name: FluctlightDB Benchmark Results
size_categories:
  - n<1K
---

# FluctlightDB — Frozen Benchmark Results

Official frozen metrics for the FluctlightDB research paper (July 2026).

## Files

| File | Description |
|------|-------------|
| `results.json` | Merged paper metrics — LoCoMo, BEIR, FAMB, LongMemEval-S |

## Key numbers

| Benchmark | Metric | Score |
|-----------|--------|------:|
| LoCoMo | Evidence recall | **99.0%** (1970/1982, k=150) |
| LongMemEval-S | session_recall@8 | **97.6%** (488/500) |
| BEIR SciFact | nDCG@10 / R@10 | **0.646 / 0.792** (CHORUS/PRISM + Fabric) |
| FAMB | Macro | **98%** index / **97%** agent |

### LongMemEval-S by type (session@8)

| Type | Score |
|------|------:|
| knowledge-update | 100% |
| multi-session | 98.5% |
| single-session-user | 98.6% |
| single-session-assistant | 98.2% |
| temporal-reasoning | 96.2% |
| single-session-preference | 96.7% |

## Reproduce

```bash
git clone https://github.com/voxmastery/FluctlightDB.git
cd FluctlightDB
# LoCoMo / BEIR / FAMB: benchmarks/README.md
# LongMemEval full 500: benchmarks/longmemeval_colab.ipynb (GPU)
```

## Paper

- DOI: https://doi.org/10.5281/zenodo.20949890
- LaTeX: https://github.com/voxmastery/FluctlightDB/tree/main/papers/arxiv-v1
- arXiv checklist: `papers/arxiv-v1/ARXIV_SUBMISSION.md`
- Card: https://huggingface.co/Voxiesz/fluctlightdb-paper

## Citation

Use [CITATION.cff](https://github.com/voxmastery/FluctlightDB/blob/main/CITATION.cff) from the main repository.
