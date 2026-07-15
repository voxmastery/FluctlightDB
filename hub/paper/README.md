---
license: mit
language: en
tags:
  - agent-memory
  - database
  - retrieval
  - llm
  - research-paper
datasets:
  - Voxiesz/fluctlightdb-benchmarks
---

# FluctlightDB: A Memory Model of Data for AI Agents

**Preprint · July 2026 · DOI: [10.5281/zenodo.20949890](https://doi.org/10.5281/zenodo.20949890) · arXiv cs.DB (pending)**

**Author:** Ganesh S · [ORCID 0009-0006-7758-4114](https://orcid.org/0009-0006-7758-4114) · voxmastery@gmail.com

## One-line claim

Long-term agent memory is a **third data model** — not SQL rows, not vector ANN alone. FluctlightDB is an embedded engine with native `experience()` / `activate()` semantics.

## Headline results

| Benchmark | Metric | Result |
|-----------|--------|--------|
| **LoCoMo** (10 conv, 1,982 gold spans) | Mean evidence recall @ k=150 | **96.8%** MiniLM / **97.0%** mpnet (no expansion; tight-k @5=72.6%/75.1%) |
| **LongMemEval-S** (500 questions) | session_recall@8 | **97.6%** (488/500 unified v4) |
| **BEIR SciFact** | nDCG@10 / R@10 | **0.646 / 0.792** (Fabric on; vs Chroma 0.645 / 0.783) |
| **FAMB** | Macro (index / agent) | **98% / 97%** |

Frozen metrics: [fluctlightdb-benchmarks](https://huggingface.co/datasets/Voxiesz/fluctlightdb-benchmarks)

## Abstract

For fifty years, data systems answered two questions: which records match a predicate (relational), and which vectors lie nearest a query (vector). Autonomous agents ask a third: *what have I learned, and what of it can I trust?*

We present **FluctlightDB**, an embedded brain-native database with write path `experience()` and read path `activate()`. On full LoCoMo (1,982 questions) its native Rust engine recalls **96.8%** of gold evidence @150 with MiniLM-384 (**97.0%** with mpnet-768), no neighbor expansion; E2E QA is retrieval-bound at ~85% @k=15. On LongMemEval-S it scores **97.6%** session recall@8 (unified v4 full 500); preference **96.7%** (29/30) with v4 pref-facts. On BEIR SciFact it edges Chroma on nDCG@10 and Recall@10 in a shared harness (Fabric on). On FAMB it scores **100%** macro (internal regression).

## Install

```bash
pip install "fluctlightdb[native]==0.5.9" "fluctlightdb-native==0.5.9"
```

```python
from fluctlightdb import connect_embedded

brain = connect_embedded("/tmp/agent-brain")
brain.experience("User prefers dark mode", context="settings", salience=0.8)
print(brain.activate("dark mode"))  # paper API; offline lexical cue
brain.checkpoint()
```

Production embedded path: [docs/EMBEDDED.md](https://github.com/voxmastery/FluctlightDB/blob/main/docs/EMBEDDED.md) · Paper metrics frozen in `paper-2026-07-09.json` (unchanged by 0.5.x SDK patches).

## Links

| Resource | URL |
|----------|-----|
| **DOI (Zenodo preprint)** | https://doi.org/10.5281/zenodo.20949890 |
| **LaTeX source** | https://github.com/voxmastery/FluctlightDB/tree/main/papers/arxiv-v1 |
| **Figure 1** | Brain directory + engram + `experience()` / `activate()` paths |
| **Figures 2–3** | Benchmark summary + LongMemEval by type — [`papers/figures/`](https://github.com/voxmastery/FluctlightDB/tree/main/papers/figures) |
| **Interactive viewer (Space)** | https://huggingface.co/spaces/Voxiesz/fluctlightdb-paper-viewer |
| **GitHub** | https://github.com/voxmastery/FluctlightDB |
| **Release v0.5.9** | https://github.com/voxmastery/FluctlightDB/releases/tag/v0.5.9 |
| **PyPI (SDK)** | https://pypi.org/project/fluctlightdb/0.5.9/ |
| **PyPI (native)** | https://pypi.org/project/fluctlightdb-native/0.5.9/ |
| **Embedded guide** | https://github.com/voxmastery/FluctlightDB/blob/main/docs/EMBEDDED.md |
| **Reproduce benchmarks** | https://github.com/voxmastery/FluctlightDB/tree/main/benchmarks |

## Citation

```bibtex
@article{s2026fluctlightdb,
  title={FluctlightDB: A Memory Model of Data for AI Agents},
  author={S, Ganesh},
  year={2026},
  doi={10.5281/zenodo.20949890},
  url={https://doi.org/10.5281/zenodo.20949890},
  note={Preprint. Software: https://github.com/voxmastery/FluctlightDB}
}
```

See also [CITATION.cff](https://github.com/voxmastery/FluctlightDB/blob/main/CITATION.cff) on GitHub.

## Metric note

LoCoMo **evidence recall** = fraction of gold dialogue evidence in retrieved context (official RAG metric). Mem0/Zep often report **LLM-as-judge end-to-end QA** — a different, harder number. Do not compare 96.8% recall to ~92% QA without naming the metric; evidence-recall ≠ QA. (The historical **99.0%** headline came from ±3 neighbor-expansion scoring, not the engine — deprecated.)

## License

MIT — engine, harnesses, and this paper draft.
