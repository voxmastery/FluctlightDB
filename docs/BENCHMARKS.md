# FluctlightDB Benchmarks (Research & Paper Use)

This document lists **trusted, citable benchmarks** for evaluating agent memory systems, what we run in-repo, and how to cite them in a research paper.

> **Verification status:** All frozen headline numbers are **maintainer-reported**. Open harnesses exist (`make reproduce-locomo`, Colab notebooks, bench scripts) but **no independent third-party reproduction has been published yet**. See [REPRODUCIBILITY.md](REPRODUCIBILITY.md) and [MAINTAINER.md](../MAINTAINER.md).

## Memory layer vs database

| | **Database** (Postgres, Chroma, Pinecone) | **Memory layer** (FluctlightDB) |
|---|---|---|
| **Primary unit** | Row, document, vector | Episode / engram (experience + context + provenance) |
| **Write semantics** | Insert / upsert | Experience → dentate separation → graph wiring → consolidation |
| **Read semantics** | SQL filter or ANN top-k | Cue-driven activation (lexical + semantic + spreading activation) |
| **Agent concerns** | You assemble recall, provenance, dedup in app code | Built-in: paraphrase recall, verified vs chat, persistence, determinism |
| **Typical use** | General storage, RAG index | Long-lived agent state across sessions |

**FluctlightDB is both:** a persisted engine (like a database) **and** a purpose-built memory layer with agent-native read/write semantics. It is not a thin SDK wrapper over a vector store.

### API modes (explicit, not env-only)

```python
from fluctlightdb import connect, connect_agent_fast, connect_index

brain = connect("/data/agent.brain")              # full agent path (episodic memory)
fast = connect_agent_fast("/data/agent.brain")    # same writes, hybrid index + 1-hop recall
index = connect_index("/data/rag.brain")          # bulk IR path (fast ingest + vector recall)
index = connect_index()                           # ephemeral, for benchmarks
```

- **`connect()` / agent mode** — dentate gate, graph co-activation, provenance ranking. Use for live agents.
- **`connect_agent_fast()`** — same write path; `FLUCTLIGHT_AGENT_FAST` + capped hybrid candidates. See [FAST_PATH.md](FAST_PATH.md).
- **`connect_index()` / index mode** — `FLUCTLIGHT_FAST_INGEST` + `FLUCTLIGHT_VECTOR_FAST`. Use for RAG backfills and IR comparisons.

---

## Tier 1: Trusted benchmarks for agent memory papers

These are widely cited in Mem0, Zep, LangMem, and recent memory-system papers.

### 1. BEIR (generic IR — credibility anchor)

| Field | Value |
|---|---|
| **What** | Standard information-retrieval benchmark suite (SciFact, NFCorpus, …) |
| **Metrics** | nDCG@10, Recall@10, Recall@100 via **pytrec_eval** vs official qrels |
| **Why cite** | Reviewers trust BEIR; Chroma/FAISS leaderboard numbers are reproducible |
| **Paper** | Thakur et al., *BEIR: A Heterogeneous Benchmark for Zero-shot Evaluation of Information Retrieval Models*, NeurIPS 2021 D&B |
| **Leaderboard** | https://github.com/beir-cellar/beir |
| **In-repo** | `benchmarks/beir_bench.py` |

```bash
pip install chromadb pytrec-eval-terrier fluctlightdb[native]
BEIR_DATA=/tmp/beir BEIR_DS=scifact MODE=index python benchmarks/beir_bench.py
```

See also `benchmarks/locomo_bench.py`, `benchmarks/longmemeval_bench.py`, and `benchmarks/README.md`.

**Reference numbers (SciFact, all-MiniLM-L6-v2, shared embeddings):**

**Paper freeze (July 2026, `FLUCTLIGHT_FABRIC=1` on CHORUS lane):**

| System | nDCG@10 | Recall@10 | Query (ms) |
|---|---:|---:|---:|
| Chroma + MiniLM | 0.645 | 0.783 | 17 |
| FluctlightDB (CHORUS/PRISM/Fabric) | **0.646** | **0.792** | 16 |

Legacy index-mode reference (pre-Fabric paper profile):

| System | write/doc | query | nDCG@10 | Recall@10 | Recall@100 |
|---|---:|---:|---:|---:|---:|
| Chroma + MiniLM | ~0.65 ms | ~10 ms | 0.645 | 0.783 | 0.925 |
| FluctlightDB (index) | ~1.0 ms | **~5 ms** | 0.645 | 0.783 | 0.925 |
| FluctlightDB (agent) | ~10 ms | ~15 ms | **0.651** | **0.790** | **0.941** |

Index-mode query latency uses slim vector-fast recalls (large doc bodies omitted from API payloads; metrics unchanged because BEIR scores by `context` doc id).

---

### 2. LoCoMo (long conversational memory)

| Field | Value |
|---|---|
| **What** | Very long multi-session dialogues; QA, event summarization, multimodal variants |
| **Metrics** | Upstream LoCoMo: gold `dia_id` in retrieved context. **Our harness (historical):** also applies `expand_session_neighbors(±3)` before scoring — see [#2](https://github.com/voxmastery/FluctlightDB/issues/2) |
| **Paper** | Maharana et al., *Evaluating Very Long-Term Conversational Memory of LLM Agents*, ACL 2024 |
| **Site** | https://snap-research.github.io/locomo/ |
| **Status** | **Honest raw: 96.8% @k=150** (2627/2823 spans, no expansion) via the first-principles invented stack in the Rust engine. **Tight-k (what a RAG app actually uses): @5 72.6%, @10 80.0%, @20 85.6%.** Frozen: `benchmarks/results/locomo-invented-stack-engine-2026-07-13.json`. The historical *99.0% expanded* is deprecated as a headline (see below). |
| **In-repo** | `benchmarks/locomo_engine_maxsim.py` (native invented stack), `benchmarks/locomo_honest.py` (2-channel prototype), `benchmarks/locomo_eval.py` (`--neighbor-window 0` for raw CHORUS lane) |

**One-command reproduce:**

```bash
make reproduce-locomo
# or: bash scripts/reproduce-locomo.sh
# dual scores (raw + expanded) are printed in JSON; --neighbor-window 0 disables expansion
```

**FluctlightDB results (July 2026) — honest raw scoring, no expansion:**

all-MiniLM-L6-v2 ONNX (384d). A gold `dia_id` counts only if that exact turn is in top-k. The
final number runs the first-principles invented stack natively in the Rust CHORUS engine
(`benchmarks/locomo_engine_maxsim.py`); the earlier 2-channel prototype is `locomo_honest.py`.

| Retrieval | raw recall@150 | Δ |
|-----------|---------------|---|
| Single-turn dense (MiniLM mean-pool cosine) | 87.5% | baseline |
| + episodic context binding (±2 neighbours in chunk) | 92.0% | +4.5 |
| + dual-pathway dense⊕BM25 RRF | 96.0% | +8.5 |
| + token-population MaxSim (late interaction) ⊕ BM25 | 96.3% | +8.8 |
| **+ first-principles invented stack** (salience-MaxSim + conjunctive surprisal + evidence fusion) | **96.8%** | **+9.3 total** |

**Recall@k profile (what a RAG app actually consumes — the tight-k numbers that matter):**

| k | 5 | 10 | 20 | 50 | 150 |
|---|---|---|---|----|-----|
| mean-pool ⊕ BM25 | 59.2% | 69.1% | 77.3% | 89.4% | 95.6% |
| MaxSim ⊕ BM25 (borrowed) | 65.9% | 74.0% | 82.0% | 91.3% | 96.9% |
| **invented stack** | **72.6%** | **80.0%** | **85.6%** | **91.8%** | **96.8%** |

**Read the tight-k row, not just @150.** @150 retrieves ~18% of a conversation's turns — a lenient
ceiling. A real RAG turn feeds only ~5–20 memories to the LLM, so **@5=72.6% / @10=80.0% is the
honest operational number**; @150=96.8% is the upper bound. The invented stack's gains are
concentrated exactly there (+6.7 @5, +6.0 @10 over the borrowed baseline).

Per-category @150: temporal 98.4 · singlehop 98.8 · adversarial 97.6 · multihop 92.3 · **opendomain 82.9** (the remaining gap — the last points to 98%+ need a stronger *base* encoder, e.g. mpnet/bge/e5; MiniLM's token vectors are the ceiling).

Two mechanisms tested and **rejected** for honesty/quality:
- `expand_session_neighbors(±3)` — inflates to 99.0% by crediting neighbours never retrieved. A trivial BM25 baseline also scores ~99% under it, so it distinguishes no engine. **Not reported as a headline.**
- CA3 pattern-completion via PRF/Rocchio query feedback — drifts on multi-topic dialogue (−1 to −2 pts). Genuine completion needs LLM-based HyDE (model access not assumed here).

Path to 98%+: the opendomain/multihop gap is semantic, not lexical — it needs a stronger retrieval embedder (bge/e5/gte-large or mpnet, per LongMemEval's 97.6%). That is the next lever, not more chunking or fusion tricks.

> Mem0/Zep often report **LLM-as-judge end-to-end QA** on LoCoMo (~92% / ~75%) — not the same metric as evidence recall. Compare only when the metric column matches.
>
> **Other benches:** BEIR uses official pytrec_eval (no neighbor expand). LongMemEval uses official session_recall@K (gold session id in top-K; no post-hoc neighbor credit). FAMB is an internal regression suite.

**BibTeX:**
```bibtex
@inproceedings{maharana2024locomo,
  title={Evaluating Very Long-Term Conversational Memory of LLM Agents},
  author={Maharana, Adyasha and others},
  booktitle={ACL},
  year={2024}
}
```

---

### 3. LongMemEval (multi-session agent abilities)

| Field | Value |
|---|---|
| **What** | 500 questions testing 6 abilities: single/multi-session, temporal, knowledge update, … |
| **Metrics** | Official **session_recall@K** (gold `answer_session_ids` in top-K); end-to-end QA with LLM judge is separate |
| **Used by** | Mem0, Zep, multiple 2024–2025 memory papers |
| **Paper** | Wu et al., *LongMemEval: Benchmarking Long-Term Memory in LLM Agents*, ICLR 2025 |
| **Status** | **Eval complete** — **97.6%** session recall@8 unified v4 full 500 (frozen `benchmarks/results/paper-2026-07-04.json`) |
| **In-repo** | `benchmarks/longmemeval_bench.py`, `benchmarks/longmemeval_colab.ipynb`, `docs/LONGMEMEVAL_ROADMAP.md` |

**FluctlightDB results (July 2026, LongMemEval-S v2, session granularity):**

| Config | session@8 | sec/q | Notes |
|--------|----------:|------:|-------|
| session + dual-key + query-expand (MiniLM) | 73.3% | ~372 | preference slice only |
| **session + dual-key + query-expand + pref-facts (mpnet, Colab GPU v2)** | **97.6%** | **8.8** | unified 500 questions 488/500 |
| ↳ preference slice | **96.7%** | — | 29/30 session@8 |

```bash
# Local (CPU embeds slow; use Colab notebook for full run)
export FLUCTLIGHT_EMBED_URL=http://127.0.0.1:8794
./scripts/start-embed-mpnet.sh
python3 benchmarks/longmemeval_bench.py \
  --granularity session --metric session \
  --dual-key --query-expand --top-k 8 --mode index
```

Leaderboard context: gbrain 97.6% R@5 (hybrid + text-embedding-3-large); YourMemory 95.8% R@5 (mpnet + BM25).

---

### 4. MemoryAgentBench (ICLR 2026)

| Field | Value |
|---|---|
| **What** | Incremental multi-turn memory: accumulation, temporal reasoning, conflict resolution |
| **Metrics** | AR (accumulative recall), TTL, LRU-style tasks |
| **Paper** | HUST-AI-HYZ, ICLR 2026 |
| **Code** | https://github.com/HUST-AI-HYZ/MemoryAgentBench |
| **Status** | Strong fit for Fluctlight (conflict / provenance / incremental ingest) |

---

### 5. MemBench (ACL 2025 Findings)

| Field | Value |
|---|---|
| **What** | Factual + reflective memory in conversational agents |
| **Paper** | ACL 2025 Findings |
| **Code** | https://github.com/import-myself/Membench |
| **Status** | Tier-1 alternative if focusing on reflection / self-model |

---

## Tier 2: Supplementary benchmarks

| Benchmark | Focus | Notes |
|---|---|---|
| **Evo-Memory / EvoMemBench** | Evolving memory under distribution shift | Good for consolidation / forgetting claims |
| **MemoryArena (2026)** | Head-to-head memory modules | Useful for related-work positioning |
| **FindingDory** | Embodied episodic memory | If claiming spatial / embodied recall |
| **Episodic Memories (Huet et al. 2025)** | Episodic structure in LLM agents | Theoretical framing |

---

## In-repo: FluctlightDB Agent Memory Benchmark (FAMB)

**Purpose:** BEIR measures generic document retrieval. FAMB measures behaviors **specific to agents** that vector DBs do not test.

| Suite | What it tests | Agent relevance |
|---|---|---|
| `paraphrase_recall@1` | Paraphrased cue → canonical episode | Real user queries ≠ stored wording |
| `provenance_top1` | Verified ledger beats chat claim | Trust / grounding |
| `persistence_recall` | Recall after checkpoint + reopen | Cross-session memory |
| `confusion_ingest` | Near-duplicate chat doesn't block new facts | Write-path separation |
| `determinism` | Same cue → same ranked engrams | Reproducible agent behavior |

```bash
pip install chromadb fluctlightdb[native]
PYTHONPATH=sdks/python python benchmarks/agent_memory_bench.py --mode agent
PYTHONPATH=sdks/python python benchmarks/agent_memory_bench.py --mode chorus --json-out /tmp/famb-chorus.json
```

**Macro score** = mean of suite scores (0–1). Report both `agent` and `chorus` modes separately.

FAMB is an **internal regression suite** (not LoCoMo-scale external validation): paraphrase $n=10$; provenance, persistence, confusion, and determinism are each one pass/fail scenario ($n=1$). CHORUS provenance/persistence suites call `chorus_sleep()` then `activate()` so durable hippocampal engrams participate (in-memory CHORUS traces alone are not checkpointed).

**Graded provenance conflicts ($n=50$):** `benchmarks/provenance_conflict_bench.py` — isolated agent brain per case scores **100%** (50/50); **shared-brain** (`--shared-brain`, all cases in one brain) scores **18%** (9/50) from cross-case cue contamination. Reproduce: `scripts/reproduce-provenance.sh`.

**LoCoMo ablations:** `benchmarks/locomo_ablation.py --sweep-k` (recall@$k$ sensitivity on CHORUS+Fabric); `--hybrid-vs-vector` (index lane hybrid vs vector-fast at fixed $k$).

**LongMemEval multi-$K$:** `longmemeval_bench.py --report-ks 5,8,10` scores session recall at $K=5,8,10$ from one recall pass at $\max K$. Merge sharded runs: `benchmarks/merge_longmemeval_shards.py`.

**Latest runs (2026-07-09, noise=200, measured suites):**

| Mode | paraphrase@1 | provenance | persistence | confusion | determinism | **MACRO** |
|---|---:|---:|---:|---:|---:|---:|
| agent | 100% | 100% | 100% | 100% | 100% | **100%** |
| chorus | 100% | 100% | 100% | 100% | 100% | **100%** |

---

## Development protocol (freeze dates)

Headline numbers in `benchmarks/results/paper-2026-07-10.json` (reviewer remediation freeze):

| Benchmark | Config frozen | Final cert JSON | Notes |
|---|---|---|---|
| LoCoMo CHORUS + Fabric | 2026-07-09 | `locomo-chorus-fabric-2026-07-09.json` | 99.0% @ $k{=}150$; k-sweep: `locomo-k-sweep-fabric-2026-07-10.json` |
| LoCoMo hybrid vs vector (index) | 2026-07-10 | `locomo-hybrid-index-2026-07-10.json` | @ $k{=}50$; hybrid $\approx$ vector |
| Provenance conflict (50 cases, isolated) | 2026-07-10 | `provenance-conflict-2026-07-10.json` | Agent lane; 100% top-1 |
| Provenance conflict (50 cases, shared brain) | 2026-07-10 | `provenance-conflict-shared-2026-07-10.json` | 18% top-1; cross-case contamination |
| BEIR SciFact PRISM + Fabric | 2026-07-09 | `beir-prism-fabric-2026-07-09.json` | Shared MiniLM; Chroma baseline in same harness |
| FAMB agent/chorus | 2026-07-09 | `famb-*-2026-07-09.json` | Replaced hardcoded CHORUS sub-scores with measured suites |
| LongMemEval-S v4 | 2026-07-04 | `longmemeval-colab-v2-full-2026-07-04.json` | Unified 500; pref-facts ablation on preference slice only during dev |
| LongMemEval E2E | 2026-07-07 | `e2e-cert-paper-v2-2026-07-07.json` | Reader/judge profile frozen before cert |

Dev iterations (07-07–07-09) were harness fixes and regression reruns on the suites above—not post-hoc edits to frozen JSON fields.

---

## Recommended paper evaluation protocol

For a credible agent-memory paper, we recommend **three layers**:

1. **IR credibility (BEIR)** — SciFact (+ optionally NFCorpus). Same embedding model for all systems. Report nDCG@10, Recall@10/100, write latency, query latency.
2. **Agent credibility (LoCoMo or LongMemEval)** — End-to-end with your agent loop; cite the original benchmark paper.
3. **Memory-specific (FAMB)** — Paraphrase, provenance, persistence; highlights FluctlightDB vs raw vector store.

### Suggested related-work sentence

> We evaluate semantic retrieval on BEIR SciFact (Thakur et al., 2021) using official qrels and pytrec_eval, long-horizon dialogue memory on LoCoMo (Maharana et al., 2024), and agent-specific recall/provenance/persistence on our FluctlightDB Agent Memory Benchmark (FAMB), which complements generic IR benchmarks with tasks aligned to episodic agent memory.

### Baselines to report

| Baseline | Role |
|---|---|
| **Chroma + same embedder** | Vector DB apples-to-apples |
| **FluctlightDB index mode** | Speed-competitive semantic index |
| **FluctlightDB agent mode** | Full memory layer (graph, provenance, separation) |
| **Mem0 / Zep** (optional) | Published agent-memory systems on LoCoMo/LongMemEval |

---

## Dependencies

```bash
pip install chromadb pytrec-eval-terrier fluctlightdb[native]
# BEIR data: manual download from UKP (see beir_bench.py header)
```

---

## Changelog

| Date | Change |
|---|---|
| 2026-07-09 | FAMB: measured CHORUS provenance/persistence suites; freeze protocol table |
| 2026-07 | LongMemEval-S: **97.6%** session@8 unified v4 (488/500); preference **96.7%** (29/30); frozen in `paper-2026-07-04.json` |
| 2025-06 | Initial BENCHMARKS.md: BEIR harness in-repo, FAMB, Tier-1 citation table, connect vs connect_index |
