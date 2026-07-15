# Benchmark reproducibility & verification status

FluctlightDB publishes benchmark numbers in the paper, README, and frozen JSON under `benchmarks/results/`. This document separates **what you can reproduce yourself** from **what has been independently verified**.

## Why independent reproduction benefits us

| Benefit | What it unlocks |
|---------|-----------------|
| **Credibility** | Moves headline metrics from “maintainer blog post” to “anyone can verify” — rare in the agent-memory space |
| **Adoption** | Enterprise / research integrators can pin versions and run `make reproduce-locomo` in CI before trusting recall claims |
| **Differentiation** | Honest `REPRODUCIBILITY.md` + open harness vs competitors’ self-reported leaderboard percentages |
| **Co-maintainers** | External reproducers are the best pipeline for triage contributors and named reviewers |
| **Head-to-head posts** | Mem0/Zep comparisons stay **blocked until** the first external LoCoMo match — then a protocol-identical post is credible, not more noise |
| **Paper & press** | “Independently reproduced by X” is citable; a self-reported number alone is not |

We welcome independent reproductions and list **public credit** in this doc (no cash bounty — maintainer-funded rewards are not offered).

## Will you get the same %?

> **Honest LoCoMo headline:** **96.8% @150 (MiniLM) / 97.0% (mpnet)** raw evidence recall, **no expansion** — tight-k @5=72.6%/75.1%, @10=80.0%/82.6%, @20=85.6%/87.2%, @50=91.8%/92.4%. Native Rust CHORUS first-principles invented stack; bench `benchmarks/locomo_engine_maxsim.py`, frozen `benchmarks/results/locomo-invented-stack-engine-2026-07-13.json` (MiniLM) / `locomo-mpnet-engine-2026-07-15.json` (mpnet). The legacy `make reproduce-locomo` protocol below reproduces the **deprecated ±3-expansion cert** (the old 99.0%), kept for historical parity only. Evidence recall ≠ QA accuracy (E2E ≈85% @k=15).

**Legacy expansion cert (deprecated) — CHORUS @ k=150: yes — when you follow the pinned protocol.**

`make reproduce-locomo` exits **0** only if your output JSON matches the frozen cert **exactly** on:

- `evidence_hits` (string, e.g. `"1970/1982"`)
- `mean_evidence_recall` (float, e.g. `0.990201277587756`)

That is the legacy **99.0%** (1970/1982) ±3-expansion cert — **deprecated, not the headline** (the honest number is 96.8% @150 no-expansion, above).

| Requirement | Why it matters |
|-------------|----------------|
| **Git tag** matching the cert (e.g. `v0.5.6`) | Recall logic lives in `fluctlightdb-native` |
| **`fluctlightdb[native]==<tag version>`** | Script pins from `crates/fluctlight-py/pyproject.toml` |
| **`benchmarks/requirements-reproduce.txt`** | Pins Chroma / eval deps (embeddings) |
| **`--mode chorus --top-k 150`** | Default in `reproduce-locomo.sh` |
| **`FLUCTLIGHT_FABRIC=1`** | Paper-profile Recall Fabric on (set by `reproduce-locomo.sh`; also via `configure_ir_env()` in `locomo_eval.py`) |
| **Frozen cert** (honest raw, no expansion) | `benchmarks/results/locomo-lateinteraction-2026-07-13.json` (self-contained); engine: `locomo-invented-stack-engine-2026-07-13.json` (MiniLM) / `locomo-mpnet-engine-2026-07-15.json` (mpnet) |
| **Official `locomo10.json`** | Auto-downloaded from snap-research/locomo |

**What can still differ (without changing the pass/fail check):** `wall_s`, embed cache hit counts, absolute paths in JSON metadata.

**What can cause a mismatch:**

| Cause | Typical symptom |
|-------|-----------------|
| Unpinned / newer **chromadb** (ONNX MiniLM drift) | `evidence_hits` off by a few spans |
| Wrong **native** version or building from arbitrary `main` | Recall ranking changes |
| Wrong lane (`agent` vs `chorus`) or `top_k` | Very different % |
| Modified dataset file | Any divergence |

**LongMemEval / BEIR:** More moving parts (mpnet server, Chroma versions, rerank flags). Exact float match is harder; report your numbers via the reproduction issue template even if not bit-identical.

**We have not yet had an external party confirm a match** — if you reproduce (or disprove) a frozen cert, use the issue template below.

---

## Leaderboards (industry context)

There is **no canonical live leaderboard** for LoCoMo or LongMemEval maintained by a neutral third party. Headline numbers from Mem0, Zep, ByteRover, Hindsight, Memobase, etc. are overwhelmingly **vendor blog posts or self-published tables**, not audited rankings.

FluctlightDB's differentiation is honesty + open harnesses:

- Frozen JSON + `make reproduce-locomo` (not just a percentage in a README)
- This doc states **self-reported until independently verified**
- **No** Mem0/Zep head-to-head post until an external reproduction lands (would add noise before that)

---

## Independent reproduction (public credit)

| Benchmark | First external reproduction | Recognition |
|-----------|----------------------------|-------------|
| LoCoMo CHORUS @150 | Not yet | Permanent credit in this doc + README |
| LongMemEval-S session@8 | Not yet | Same |
| BEIR SciFact nDCG@10 | Not yet | Same |
| FAMB macro | Not yet | Same |

LongMemEval E2E is **excluded** (locked maintainer run; OpenAI cost).

**How to report:** open a [**Benchmark reproduction** issue](https://github.com/voxmastery/FluctlightDB/issues/new?template=reproduction.yml), include environment + output JSON. Maintainer verifies and updates the table below.

### Confirmed external reproductions

| Date | Who | Benchmark | Result | Match |
|------|-----|-----------|--------|-------|
| — | *none yet* | — | — | — |

---

## Verification status (honest summary)

| Status | Meaning |
|--------|---------|
| **Open harness** | Script/notebook + frozen JSON exist in this repo; anyone can re-run |
| **Self-reported** | Numbers were produced by the maintainer; no third party has published a matching run |
| **Locked** | Re-run is intentionally frozen (cost, API keys, or paper freeze) |

| Benchmark | Frozen artifact | Open harness | Maintainer self-reported | Independent third-party reproduction |
|-----------|-----------------|--------------|--------------------------|-------------------------------------|
| LoCoMo evidence recall @150 (honest raw, no expansion) | `locomo-invented-stack-engine-2026-07-13.json` (MiniLM), `locomo-mpnet-engine-2026-07-15.json` (mpnet) | `benchmarks/locomo_engine_maxsim.py` | Yes — **96.8% @150 (MiniLM) / 97.0% (mpnet)**, @5=72.6%/75.1% tight-k | **None published** |
| LongMemEval-S session@8 | `paper-2026-07-09.json` | Colab + local scripts | Yes — **97.6%** | **None published** |
| LongMemEval E2E QA | `e2e-cert-paper-v2-2026-07-07.json` | `benchmarks/e2e_certify.sh` (OpenAI) | Yes — **97.4%** | **None** — run locked |
| BEIR SciFact nDCG@10 / R@10 | `paper-2026-07-09.json` | `benchmarks/beir_bench.py` | Yes — **0.646 / 0.792** (Fabric on) | **None published** |
| FAMB macro | `paper-2026-07-09.json` | `benchmarks/famb_bench.py` | Yes — **100%** | **None published** |

**Bottom line:** Harnesses are open and numbers are frozen, but **all headline metrics are maintainer-reported until an external group publishes a reproduction** (issue, blog, paper, or fork).

## Reproduce LoCoMo (fastest independent check)

```bash
git clone https://github.com/voxmastery/FluctlightDB.git && cd FluctlightDB
git checkout v0.5.6   # or tag matching the frozen cert you compare against
make reproduce-locomo
```

This downloads LoCoMo data, runs the honest self-contained recipe (`benchmarks/locomo_lateinteraction.py` — token-population MaxSim ⊕ BM25, no neighbor expansion), and compares raw recall@150 against `locomo-lateinteraction-2026-07-13.json`. The native-engine invented-stack headline (**96.8% @150 MiniLM / 97.0% mpnet, no expansion**) reproduces via `benchmarks/locomo_engine_maxsim.py` after a source build, comparing against `locomo-invented-stack-engine-2026-07-13.json` / `locomo-mpnet-engine-2026-07-15.json`. The old ±3-expansion 99.0% is deprecated (a trivial BM25 baseline also scored ~99% under it).

Options:

```bash
REPRODUCE_FROM_SOURCE=1 make reproduce-locomo   # build native wheel from source
OUT=benchmarks/results/my-run.json bash scripts/reproduce-locomo.sh
pip install -r benchmarks/requirements-reproduce.txt   # deps only
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

Use the [**Benchmark reproduction** issue template](https://github.com/voxmastery/FluctlightDB/issues/new?template=reproduction.yml) (label: `reproduction`).

Include: commit hash, OS, Python version, `fluctlightdb` / `fluctlightdb-native` versions, output JSON path, diff vs frozen cert.

Partial reproductions (e.g. within ±0.5% due to embedding drift) are still valuable — report them.

## What we do not claim

- Peer review of benchmark methodology (paper is preprint)
- Independent audit of correctness or security
- Leaderboard registration on a third-party site — see [LEADERBOARD.md](LEADERBOARD.md) (no neutral agent-memory registry exists)
