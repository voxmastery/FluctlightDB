# Benchmarks

Run from repo root unless noted. Install deps:

```bash
pip install chromadb pytrec-eval-terrier "fluctlightdb[native]"
# or dev: pip install -e sdks/python && ./scripts/install-native.sh
```

| Script | Purpose | Data |
|--------|---------|------|
| `beir_bench.py` | Certified IR (nDCG@10, Recall@10/100) | [BEIR SciFact](https://public.ukp.informatik.tu-darmstadt.de/thakur/BEIR/datasets/scifact.zip) |
| `agent_memory_bench.py` | Agent-specific: paraphrase, provenance, persistence | Built-in |
| `locomo_bench.py` | LoCoMo long-dialogue evidence recall | [LoCoMo](https://snap-research.github.io/locomo/) |
| `longmemeval_bench.py` | LongMemEval session evidence recall | [LongMemEval](https://github.com/xiaowu0162/LongMemEval) |

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
