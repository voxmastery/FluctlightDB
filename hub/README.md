---
title: FluctlightDB
emoji: 🧠
colorFrom: blue
colorTo: purple
sdk: docker
pinned: false
license: mit
---

# FluctlightDB

**Embedded memory engine for AI agents** — a third data model (`experience()` / `activate()`), not SQL, not a vector DB, not a Mem0-style layer.

## Mission

Become **SQLite for agent memory**: one durable store per agent, cue-driven recall, provenance (verified sources beat chat), public benchmarks.

## Install

```bash
pip install "fluctlightdb[native]"
```

## Results (June 2026)

| Benchmark | Result |
|-----------|--------|
| LoCoMo evidence recall | **98.1%** (10 conv, k=150) |
| BEIR SciFact nDCG@10 | **0.645** (ties Chroma + MiniLM) |
| FAMB macro | **97–98%** |

## Links

- [GitHub](https://github.com/voxmastery/FluctlightDB)
- [PyPI](https://pypi.org/project/fluctlightdb/)
- [Paper card (HF)](https://huggingface.co/Voxiesz/fluctlightdb-paper)
- [Benchmarks dataset](https://huggingface.co/datasets/Voxiesz/fluctlightdb-benchmarks)
- [Paper viewer Space](https://huggingface.co/spaces/Voxiesz/fluctlightdb-paper-viewer)
- [Paper source (LaTeX)](https://github.com/voxmastery/FluctlightDB/tree/main/papers/arxiv-v1)
- [Zenodo DOI](https://doi.org/10.5281/zenodo.20949890)
