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
  - beir
  - benchmarks
pretty_name: FluctlightDB Benchmark Results
size_categories:
  - n<1K
---

# FluctlightDB — Frozen Benchmark Results

Official frozen metrics for the FluctlightDB research paper (June 2025).

## Files

| File | Description |
|------|-------------|
| `results.json` | Full benchmark output — LoCoMo, BEIR SciFact, FAMB |

## Key numbers

- **LoCoMo evidence recall:** 98.1% (1925/1982 gold spans, k=150, hybrid)
- **BEIR SciFact nDCG@10:** 0.645 (index mode, ties Chroma)
- **FAMB macro:** 98% index / 97% agent

## Reproduce

```bash
git clone https://github.com/voxmastery/FluctlightDB.git
cd FluctlightDB
# See benchmarks/README.md and docs/BENCHMARKS.md
```

## Paper

- Draft: https://voxmastery.github.io/FluctlightDB/paper/
- Card: https://huggingface.co/voxmastery/fluctlightdb-paper

## Citation

Use [CITATION.cff](https://github.com/voxmastery/FluctlightDB/blob/main/CITATION.cff) from the main repository.
