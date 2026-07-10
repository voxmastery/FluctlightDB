# FluctlightDB adoption roadmap — default brain for AI agents

This document tracks the **adoption layer** (integrations, DX, proof) on top of the memory engine.

## Frozen benchmark badges

| Benchmark | Metric | Result | Artifact |
|-----------|--------|--------|----------|
| LoCoMo | Evidence recall | **99.0%** | `benchmarks/results/locomo-chorus-fabric-2026-07-09.json` |
| LongMemEval-S | Session@8 retrieval | **97.6%** | `benchmarks/results/longmemeval-muon-final-2026-07-06.json` |
| BEIR SciFact | nDCG@10 | **0.645** (CHORUS/PRISM parity) | `benchmarks/results/beir-prism-prod-2026-07-08.json` |
| FAMB | Macro accuracy | **97–98%** | `benchmarks/results/famb-*-2026-07-06.json` |
| LongMemEval E2E | QA accuracy | Run `benchmarks/e2e_certify.sh` | Requires API key |

## Tier 1 — Framework drop-in (shipped)

```bash
pip install "fluctlightdb[langchain,native]"
pip install "fluctlightdb[llamaindex,native]"
```

```python
from fluctlightdb import connect_agent
from fluctlightdb.integrations import get_langchain_memory

brain = connect_agent("/data/agent.brain")
memory = get_langchain_memory(brain)
```

See [INTEGRATIONS.md](INTEGRATIONS.md).

## Tier 2 — Agent ergonomics (shipped)

- `connect_agent()` — unified recall, WM-Ring, auto-consolidate
- **Reflex** auto-ingest: `fluctlightdb.reflex.reflex_ingest_turn()`
- **Chronos** temporal gate on `recall()` (`tick_from` / `tick_to` + NL cues)
- **MCP memory tools**: `memory_remember`, `memory_recall`, `memory_resolve`, `memory_consolidate`
- **Brain inspect UI**: `fluctlight-project inspect --brain PATH`

## Tier 3 — Ops & governance (shipped)

- **Brain Snapshot** interchange: `brain.export_snapshot()` / `import_snapshot()`
- **Governance**: `scrub_pii()`, `delete_by_subject()`, `audit_log()`
- **Replica sync**: `fluctlight-project replicate PRIMARY REPLICA`
- **Git team sync**: `fluctlight-project sync pull|push`

## Leaderboard submission checklist

1. Freeze JSON under `benchmarks/results/`
2. Run E2E: `OPENAI_API_KEY=... benchmarks/e2e_certify.sh`
3. Cite DOI [10.5281/zenodo.20949890](https://doi.org/10.5281/zenodo.20949890)
4. Link this file + [BENCHMARKS.md](BENCHMARKS.md)
