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

**Preprint · June 2025 · arXiv cs.DB (submission pending)**

**Author:** Ganesh S · [ORCID 0009-0006-7758-4114](https://orcid.org/0009-0006-7758-4114) · voxmastery@ambugo.tech

## One-line claim

Long-term agent memory is a **third data model** — not SQL rows, not vector ANN alone. FluctlightDB is an embedded engine with native `experience()` / `activate()` semantics.

## Headline results

| Benchmark | Metric | Result |
|-----------|--------|--------|
| **LoCoMo** (10 conv, 1,982 gold spans) | Mean evidence recall @ k=150 | **98.1%** |
| **BEIR SciFact** | nDCG@10 | **0.645** (ties Chroma + MiniLM) |
| **FAMB** | Macro (index / agent) | **98% / 97%** |

Frozen metrics: [fluctlightdb-benchmarks](https://huggingface.co/datasets/Voxiesz/fluctlightdb-benchmarks)

## Abstract

For fifty years, data systems answered two questions: which records match a predicate (relational), and which vectors lie nearest a query (vector). Autonomous agents ask a third: *what have I learned, and what of it can I trust?*

We present **FluctlightDB**, an embedded brain-native database with write path `experience()` and read path `activate()`. On full LoCoMo it recalls **98.1%** of gold evidence (warm and cold-start identical). On BEIR SciFact it matches a tuned Chroma baseline; on FAMB it scores 97–98% macro.

## Install

```bash
pip install "fluctlightdb[native]"
```

```python
from fluctlightdb import connect

brain = connect("/tmp/agent-brain")
brain.experience("User prefers dark mode", context="settings", salience=0.8)
print(brain.activate("theme preference"))
brain.checkpoint()
```

## Links

| Resource | URL |
|----------|-----|
| **LaTeX source** | https://github.com/voxmastery/FluctlightDB/tree/main/papers/arxiv-v1 |
| **Interactive viewer (Space)** | https://huggingface.co/spaces/Voxiesz/fluctlightdb-paper-viewer |
| **GitHub** | https://github.com/voxmastery/FluctlightDB |
| **PyPI** | https://pypi.org/project/fluctlightdb/ |
| **Venue plan** | https://github.com/voxmastery/FluctlightDB/blob/main/docs/RESEARCH_VENUES.md |
| **Reproduce benchmarks** | https://github.com/voxmastery/FluctlightDB/tree/main/benchmarks |

## Citation

```bibtex
@article{s2025fluctlightdb,
  title={FluctlightDB: A Memory Model of Data for AI Agents},
  author={S, Ganesh},
  year={2025},
  note={Preprint. Software: https://github.com/voxmastery/FluctlightDB},
  url={https://github.com/voxmastery/FluctlightDB/tree/main/papers/arxiv-v1}
}
```

See also [CITATION.cff](https://github.com/voxmastery/FluctlightDB/blob/main/CITATION.cff) on GitHub.

## Metric note

LoCoMo **evidence recall** = fraction of gold dialogue evidence in retrieved context (official RAG metric). Mem0/Zep often report **LLM-as-judge end-to-end QA** — a different, harder number. Do not compare 98.1% recall to ~92% QA without naming the metric.

## License

MIT — engine, harnesses, and this paper draft.
