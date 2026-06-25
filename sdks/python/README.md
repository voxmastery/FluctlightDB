# FluctlightDB

**The memory engine for AI agents** — not a vector database with an agent SDK bolted on.

[![PyPI](https://pypi.org/project/fluctlightdb/)](https://pypi.org/project/fluctlightdb/) · [GitHub](https://github.com/voxmastery/FluctlightDB) · [Paper](https://search.ambugo.help/paper/)

## Mission

**Goal:** become the default **embedded memory substrate** for AI agents — the way SQLite became the default embedded DB for apps.

Long-term agent memory is a **third data model** (alongside relational facts and vector similarity). FluctlightDB defines engine-level `experience()` / `activate()` semantics — episodes, cue-driven recall, provenance, consolidation — not app glue on top of Chroma or Mem0.

**Who it's for:** coding agents, ops bots, research assistants, NPCs — any agent that must **remember across sessions** and **trust ledgers/files over chat**.

## Install

```bash
pip install "fluctlightdb[native]"
```

```python
from fluctlightdb import connect

brain = connect("/tmp/my-agent-brain")
brain.experience("User prefers dark mode", context="settings", salience=0.8)
print(brain.activate("theme preference"))
brain.checkpoint()
```

HTTP-only (no Rust extension): `pip install fluctlightdb`

## Benchmarks (June 2025)

| Benchmark | Metric | Result |
|-----------|--------|--------|
| **LoCoMo** (10 conv) | Mean evidence recall @ k=150 | **98.1%** |
| **BEIR SciFact** | nDCG@10 (index mode) | **0.645** (ties Chroma + MiniLM) |
| **FAMB** | Macro (index / agent) | **98%** / **97%** |

Frozen JSON: [benchmarks/results/2025-06-22.json](https://github.com/voxmastery/FluctlightDB/blob/main/benchmarks/results/2025-06-22.json)

> LoCoMo **evidence recall** ≠ Mem0 **LLM-as-judge QA** — different metrics; compare only when labeled.

## Docs

- [Getting started](https://github.com/voxmastery/FluctlightDB/blob/main/docs/GETTING_STARTED.md)
- [Full README & reproduction](https://github.com/voxmastery/FluctlightDB)
- [Platform checklist](https://github.com/voxmastery/FluctlightDB/blob/main/docs/PLATFORMS.md)
