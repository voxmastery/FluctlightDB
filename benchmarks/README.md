# Benchmarks

Run from repo root unless noted. Install deps:

```bash
pip install chromadb pytrec-eval-terrier "fluctlightdb[native]>=0.5.2"
# or dev: pip install -e sdks/python && ./scripts/install-native.sh

# LoCoMo one-command reproduce (downloads data, checks frozen cert)
make reproduce-locomo
```

| Script | Purpose | Data |
|--------|---------|------|
| `beir_bench.py` | Certified IR (nDCG@10, Recall@10/100) | [BEIR SciFact](https://public.ukp.informatik.tu-darmstadt.de/thakur/BEIR/datasets/scifact.zip) |
| `agent_memory_bench.py` | Agent-specific: paraphrase, provenance, persistence | Built-in |
| `locomo_bench.py` | LoCoMo long-dialogue evidence recall | [LoCoMo](https://snap-research.github.io/locomo/) |
| `longmemeval_bench.py` | LongMemEval session recall (`--mode brain` default) | [LongMemEval](https://github.com/xiaowu0162/LongMemEval) |
| `longmemeval_e2e.py` | LongMemEval E2E (brain retrieve → reader → judge) | Same + API key |
| `brain_memory.py` | Brain-native ingest (CLS sleep, fact engrams, completion) | (library) |

### LongMemEval modes

| Mode | API | Use |
|------|-----|-----|
| **`brain`** (default) | `connect_brain()` | Full agent path: dentate separation, graph spread, CLS sleep, cortex boost, fact/turn engrams, pattern completion |
| `conv` | `connect_conv()` | Hybrid RAG (LoCoMo-style), no sleep |
| `index` | `connect_index()` | IR-only vector-fast (Chroma-class); **not** the agent brain — legacy retrieval baseline |

E2E profiles: **`brain`** (CLS + Chain-of-Note + GPT-5), `max` (GPT-5 + CoT + top-50), `standard` (gpt-4o).

E2E full 500: `bash scripts/run-longmemeval-e2e-500.sh` — see **[docs/LONGMEMEVAL_E2E.md](../docs/LONGMEMEVAL_E2E.md)**.

Paper citations and protocol: **[docs/BENCHMARKS.md](../docs/BENCHMARKS.md)**.

```bash
# BEIR
BEIR_DATA=/tmp/beir BEIR_DS=scifact MODE=index PYTHONPATH=sdks/python python benchmarks/beir_bench.py

# FAMB
PYTHONPATH=sdks/python python benchmarks/agent_memory_bench.py --mode agent

# LoCoMo / LongMemEval (after data download)
LOCOMO_DATA=/tmp/locomo PYTHONPATH=sdks/python python benchmarks/locomo_bench.py
LONGMEMEVAL_DATA=/tmp/LongMemEval/data PYTHONPATH=sdks/python python benchmarks/longmemeval_bench.py
```
