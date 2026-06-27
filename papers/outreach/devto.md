---
title: "FluctlightDB: SQLite for Agent Memory (98.1% LoCoMo Evidence Recall)"
published: false
tags: ai, agents, database, opensource, machinelearning
canonical_url: https://voxmastery.github.io/FluctlightDB/
---

Every agent today remembers by stuffing context windows and vector stores. That is lookup, not memory.

**FluctlightDB** is an embedded database engine where the native operations are memory operations: `experience()` to encode, `activate()` to recall from a cue, `checkpoint()` to persist. Not SQL. Not a vector DB wrapper. Not a Mem0-style chat extraction layer.

## The claim

Long-term agent memory is a **third data model**:

| Model | Native question |
|-------|-----------------|
| Relational | Which rows match? |
| Vector | What's similar? |
| **Memory** | What did the agent learn, and what should recall return for this cue? |

## Numbers (frozen, reproducible)

| Benchmark | Result |
|-----------|--------|
| LoCoMo evidence recall (full 10 conv) | **98.1%** |
| BEIR SciFact nDCG@10 | **0.645** (ties Chroma) |
| FAMB macro | **97–98%** |

Metric note: LoCoMo **evidence recall** measures whether gold dialogue evidence appears in retrieved context — the official RAG metric. It is not the same as end-to-end LLM QA scores some memory layers report.

## Try it

```bash
pip install "fluctlightdb[native]"
```

```python
from fluctlightdb import connect
brain = connect("/tmp/my-agent")
brain.experience("User prefers dark mode", context="settings", salience=0.8)
print(brain.activate("theme preference"))
brain.checkpoint()
```

## Read the paper

Full preprint draft: **https://voxmastery.github.io/FluctlightDB/**

- GitHub: https://github.com/voxmastery/FluctlightDB
- Hugging Face: https://huggingface.co/voxmastery/fluctlightdb-paper
- PyPI: https://pypi.org/project/fluctlightdb/

MIT license. Benchmark harnesses included.

What would you want from an agent memory *engine* that vector DBs don't give you?
