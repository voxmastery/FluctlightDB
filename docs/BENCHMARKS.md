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
| **Metrics** | QA F1 / accuracy, summarization quality; official RAG **evidence recall** (gold `dia_id` in context) |
| **Paper** | Maharana et al., *Evaluating Very Long-Term Conversational Memory of LLM Agents*, ACL 2024 |
| **Site** | https://snap-research.github.io/locomo/ |
| **Status** | **Full eval complete** — **99.0%** mean evidence recall (10 conv, k=150, CHORUS); frozen in `benchmarks/results/locomo-chorus-2026-07-08.json` |
| **In-repo** | `benchmarks/locomo_eval.py`, `benchmarks/locomo_metrics.py` |

**One-command reproduce:**

```bash
make reproduce-locomo
# or: bash scripts/reproduce-locomo.sh
```

**FluctlightDB results (July 2026, CHORUS certified):**

| Run | Mean evidence recall | Evidence hits | Wall time |
|-----|---------------------|---------------|-----------|
| Certified (embed cache warm) | **99.0%** | 1970/1982 | ~20s |

Config: `connect_chorus()`, batch imprint per turn, `--top-k 150`, all-MiniLM-L6-v2 ONNX (via Chroma embedder in harness). For k>100, engine uses full SPECTRUM readout (PRISM certify cap).

Historical index-mode run (June 2026): 98.1% — superseded by CHORUS cert; see `benchmarks/results/2025-06-22.json`.

> Mem0/Zep often report **LLM-as-judge end-to-end QA** on LoCoMo (~92% / ~75%) — not the same metric as evidence recall. Compare only when the metric column matches.

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
| **BEAM (ICLR 2026)** | 128K–10M token chats; LLM-rubric E2E | Experimental harness: `benchmarks/beam_eval.py`, `make reproduce-beam-smoke` — **not in paper freeze** (smoke scores too low to cite pre-arXiv) |
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

**Latest runs (2026-07-09, noise=200, measured suites):**

| Mode | paraphrase@1 | provenance | persistence | confusion | determinism | **MACRO** |
|---|---:|---:|---:|---:|---:|---:|
| agent | 100% | 100% | 100% | 100% | 100% | **100%** |
| chorus | 100% | 100% | 100% | 100% | 100% | **100%** |

---

## Development protocol (freeze dates)

Headline numbers in `benchmarks/results/paper-2026-07-09.json` were frozen as follows:

| Benchmark | Config frozen | Final cert JSON | Notes |
|---|---|---|---|
| LoCoMo CHORUS | 2026-07-08 | `locomo-chorus-2026-07-08.json` | No dev-slice tuning on test convs |
| BEIR SciFact PRISM | 2026-07-08 | `beir-prism-prod-2026-07-08.json` | Shared MiniLM; Chroma baseline in same harness |
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
