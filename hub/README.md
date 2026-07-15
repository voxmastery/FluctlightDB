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
pip install "fluctlightdb[native]==0.5.9"
```

## Results (July 2026, frozen)

| Benchmark | Result |
|-----------|--------|
| LoCoMo evidence recall @150 | **96.8%** MiniLM-384 / **97.0%** mpnet-768 (native Rust engine, no expansion; tight-k @5=72.6%/75.1%) |
| LongMemEval-S session@8 | **97.6%** (488/500) |
| LongMemEval-S E2E QA | **97.4%** (487/500) |
| BEIR SciFact nDCG@10 / R@10 | **0.646 / 0.792** (Fabric on) |
| FAMB macro | **100%** (internal regression) |

## Links

- [Release v0.5.9](https://github.com/voxmastery/FluctlightDB/releases/tag/v0.5.9)
- [Embedded production guide](https://github.com/voxmastery/FluctlightDB/blob/main/docs/EMBEDDED.md)
- [PyPI](https://pypi.org/project/fluctlightdb/)
- [Paper card (HF)](https://huggingface.co/Voxiesz/fluctlightdb-paper)
- [Benchmarks dataset](https://huggingface.co/datasets/Voxiesz/fluctlightdb-benchmarks)
- [Paper viewer Space](https://huggingface.co/spaces/Voxiesz/fluctlightdb-paper-viewer)
- [Paper source (LaTeX)](https://github.com/voxmastery/FluctlightDB/tree/main/papers/arxiv-v1)
- [Zenodo DOI](https://doi.org/10.5281/zenodo.20949890)
