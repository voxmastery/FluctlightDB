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

## Results (June 2025)

| Benchmark | Result |
|-----------|--------|
| LoCoMo evidence recall | **98.1%** (10 conv, k=150) |
| BEIR SciFact nDCG@10 | **0.645** (ties Chroma + MiniLM) |
| FAMB macro | **97–98%** |

## Links

- [GitHub](https://github.com/voxmastery/FluctlightDB)
- [Paper draft](https://search.ambugo.help/paper/)
- [PyPI](https://pypi.org/project/fluctlightdb/)
- [Benchmarks JSON](https://github.com/voxmastery/FluctlightDB/blob/main/benchmarks/results/2025-06-22.json)
