# Benchmark reproducibility & verification status

FluctlightDB publishes benchmark numbers in the paper, README, and frozen JSON under `benchmarks/results/`. This document separates **what you can reproduce yourself** from **what has been independently verified**.

## Verification status (honest summary)

| Status | Meaning |
|--------|---------|
| **Open harness** | Script/notebook + frozen JSON exist in this repo; anyone can re-run |
| **Self-reported** | Numbers were produced by the maintainer; no third party has published a matching run |
| **Locked** | Re-run is intentionally frozen (cost, API keys, or paper freeze) |

| Benchmark | Frozen artifact | Open harness | Maintainer self-reported | Independent third-party reproduction |
|-----------|-----------------|--------------|--------------------------|-------------------------------------|
| LoCoMo evidence recall @150 | `locomo-chorus-2026-07-08.json` | `make reproduce-locomo` | Yes — **99.0%** | **None published** |
| LongMemEval-S session@8 | `paper-2026-07-09.json` | Colab + local scripts | Yes — **97.6%** | **None published** |
| LongMemEval E2E QA | `e2e-cert-paper-v2-2026-07-07.json` | `benchmarks/e2e_certify.sh` (OpenAI) | Yes — **97.4%** | **None** — run locked |
| BEIR SciFact nDCG@10 | `paper-2026-07-09.json` | `benchmarks/beir_bench.py` | Yes — **0.645** | **None published** |
| FAMB macro | `paper-2026-07-09.json` | `benchmarks/famb_bench.py` | Yes — **100%** | **None published** |

**Bottom line:** Harnesses are open and numbers are frozen, but **all headline metrics are maintainer-reported until an external group publishes a reproduction** (issue, blog, paper, or fork).

## Reproduce LoCoMo (fastest independent check)

```bash
git clone https://github.com/voxmastery/FluctlightDB.git && cd FluctlightDB
make reproduce-locomo
```

This downloads LoCoMo data, runs CHORUS eval, and compares against the frozen cert (`locomo-chorus-2026-07-08.json`). Expected: **1970/1982** evidence hits at k=150.

Options:

```bash
REPRODUCE_FROM_SOURCE=1 make reproduce-locomo   # build native wheel from source
OUT=benchmarks/results/my-run.json bash scripts/reproduce-locomo.sh
```

## Other benchmarks

| Benchmark | How to reproduce | Notes |
|-----------|------------------|-------|
| LongMemEval-S | `benchmarks/longmemeval_colab_v2.ipynb` or local script in `benchmarks/README.md` | Requires embedding server (mpnet) for local path |
| LongMemEval E2E | `E2E_PROFILE=paper benchmarks/e2e_certify.sh` | **OpenAI API cost**; maintainer locked paper run — do not overwrite `e2e-cert-paper-v2-2026-07-07.json` |
| BEIR SciFact | `benchmarks/beir_bench.py` | Chroma + pytrec_eval deps |
| Full paper bundle | `benchmarks/results/paper-2026-07-09.json` | Aggregates frozen runs |

Full protocol: [BENCHMARKS.md](BENCHMARKS.md) · [benchmarks/README.md](../benchmarks/README.md)

## Metric caveats (read before comparing to Mem0/Zep)

- **LoCoMo evidence recall** ≠ **LLM-judge E2E QA** used by some memory SDK leaderboards. Different task, different numbers.
- Embeddings: paper uses `all-MiniLM-L6-v2` (ONNX CPU) unless noted; LongMemEval v4 path uses mpnet.
- CHORUS lane uses bulk imprint (`connect_chorus()`); agent lane uses `connect_agent()`.

## Report your reproduction

If you independently reproduce (or fail to reproduce) a frozen number:

1. Open a [GitHub Issue](https://github.com/voxmastery/FluctlightDB/issues) titled `Reproduction: <benchmark> <your result>`
2. Include: commit hash, OS, Python version, `fluctlightdb` / `fluctlightdb-native` versions, output JSON path, diff vs frozen cert
3. We will link confirmed external reproductions from this doc and `MAINTAINER.md`

Partial reproductions (e.g. within ±0.5% due to embedding nondeterminism) are still valuable — report them.

## What we do not claim

- Peer review of benchmark methodology (paper is preprint)
- Independent audit of correctness or security
- Leaderboard registration on a third-party site
