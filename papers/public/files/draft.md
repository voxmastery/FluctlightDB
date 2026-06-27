# FluctlightDB: A Memory Model of Data for AI Agents

**A brain-native database engine for long-term agent memory**

**Author:** Ganesh S — Independent Researcher · voxmastery@ambugo.tech  
**ORCID:** [0009-0006-7758-4114](https://orcid.org/0009-0006-7758-4114)  
**Date:** June 2025 · **Draft:** arxiv-v1

## Abstract

For fifty years, data systems have answered two questions. The relational model asked *which records match a predicate*; the vector model asked *which vectors lie nearest a query*. Neither was built to answer the question an autonomous agent asks every time it wakes up: *what have I learned, and what of it can I trust?*

We argue that long-term agent memory is not an application built on top of a database — it is a **third data model**, with its own write semantics (encoding, separation, consolidation, provenance) and its own read semantics (cue-driven activation across a linked memory graph). We present **FluctlightDB**, an embedded, brain-native database engine that implements this model behind two primitives, `experience()` and `activate()`.

On the official LoCoMo long-conversation benchmark (10 conversations, 1,982 gold evidence spans), FluctlightDB's hybrid index recalls **98.1%** of gold evidence — warm and cold-start identical. On BEIR SciFact it matches a tuned Chroma + MiniLM baseline on nDCG@10 at equal latency, while agent mode improves Recall@100. On an agent-specific suite (FAMB) it scores 97–98% macro. The engine, harnesses, and frozen metrics are released under MIT. We claim no new neuroscience and no new transformer; we claim a missing layer of the data stack, and an engine that fills it.

## 1. Introduction

Every generation of software gets the database it deserves. Business records gave us the relational model and SQL. Embeddings gave us vector databases and approximate nearest-neighbor search. Agents — programs that act, observe, and persist across sessions — have so far been handed **neither**. They are stateless between runs unless a developer hand-assembles a session store, a vector index, a deduplicator, a trust policy, and glue code to keep them consistent.

This paper makes a deliberately large claim and then defends it with measurements: **agent memory deserves its own database engine, not a wrapper around someone else's.** A relational engine is the wrong abstraction because memory is not typed rows joined by keys. A vector engine is the wrong abstraction because recall is not cosine similarity alone — a fact is retrieved because it was *learned*, *linked* to a context, and *trusted*, not merely because its embedding is close.

**Contributions:**

- **A data model.** Agent memory as a first-class model of data, distinct from the relational and vector models.
- **An engine.** FluctlightDB — embedded Rust, `experience()` / `activate()` / `checkpoint()`, one durable store per agent.
- **Evidence.** 98.1% evidence recall on full LoCoMo, parity with a tuned vector baseline on BEIR, 97–98% macro on an agent-specific benchmark.
- **Reproducibility.** Open harnesses and frozen result JSON; every number re-runs with one command.

## 2. The Third Data Model

**Why rows are not memory.** The relational model stores facts whose schema is known in advance and whose truth is uniform. Agent memory is the opposite: heterogeneous, out of order, often contradictory (a chat claim vs a ledger entry), valuable *because* of where it came from. SQL has no native provenance-weighted recall.

**Why nearest-neighbor is not recall.** Vector search answers "what is similar." Memory answers "what is relevant given who I am and what I was doing." Two memories with distant embeddings can be the right answer because they co-activated in the same episode; a near embedding can be wrong because it is an unverified rumor.

**The model.** A memory store is a set of **engrams** — each with content, encoding context, salience, optional provenance, and edges to co-activated engrams. `experience` performs pattern *separation*, encodes the engram, registers its vector, and wires edges. `activate` takes a cue, seeds lexical and semantic indexes, spreads activation through the graph, fuses scores, and boosts verified sources. Consolidation replays and compacts offline. The neuroscience vocabulary is explanatory, not required.

## 3. System Design

**Embedded, one store per agent.** Like SQLite, FluctlightDB is a library, not a server. Each agent owns a directory; `checkpoint()` commits state. Nothing to provision, nothing to sync.

**Write path.** Separation gating → dentate-style encoding → semantic vector registration → graph wiring. Fast-ingest mode skips graph work for bulk indexing.

**Read path.** `activate(cue)` seeds BM25 + vector indexes, spreads activation, fuses scores, applies provenance boosts. Hybrid retrieval (vector top-k + lexical seeds) for conversational RAG.

**Two modes.** `connect()` = full episodic engine for live agents; `connect_index()` = bulk semantic path for RAG/IR. One engine, one file format.

## 4. Evaluation

All experiments use `all-MiniLM-L6-v2` (ONNX CPU). Every number is reproduced by a script in `benchmarks/` and frozen in `benchmarks/results/2025-06-22.json`.

### 4.1 BEIR SciFact — parity with a tuned vector baseline

| System | nDCG@10 | R@10 | R@100 | Query (ms) |
|--------|---------|------|-------|------------|
| Chroma | 0.645 | 0.783 | 0.925 | 5–7 |
| FluctlightDB (index) | 0.645 | 0.783 | 0.925 | 5–7 |
| FluctlightDB (agent) | **0.651** | **0.790** | **0.941** | 15 |

Index mode is a dead heat — the memory engine costs nothing as a plain vector store. Agent mode, with graph co-activation, improves deep recall.

### 4.2 LoCoMo — 98.1% evidence recall on the full set

Official evidence-recall metric: fraction of gold `dia_id` spans in retrieved context. Config: `connect_index()`, dialog + observations, k=150, hybrid vector+BM25, 2 CPU threads. Robust to cold start — the property a production memory engine must have.

| Metric | Warm | Cold |
|--------|------|------|
| Mean evidence recall | **98.1%** | **98.1%** |
| All evidence in context | 97.1% | 97.1% |
| Evidence hits | 1925/1982 | 1925/1982 |
| Wall time (s) | 271 | 335 |

We separate retrieval from generation. A verbatim answer-in-context proxy sits near 38% — but that measures string overlap, not correctness (gold answers are inferred facts, not quotes). A 50-question end-to-end pilot scored 23.5% category F1 at 99.5% retrieval; full QA awaits inference quota. The engine's job is to put evidence in front of the reader, and at that it scores 98.1%.

### 4.3 FAMB — the agent-specific suite

Paraphrase recall@1, provenance top-1, persistence, confusion ingest, determinism. Index macro **98%**, agent macro **97%**.

### 4.4 LongMemEval-S

Answer-in-recall@8 (retrieval-only). Pilot n=20: 70%. Full n=500 deferred — CPU-bound (~30s/question), to be run throttled rather than reported prematurely.

## 5. Discussion

**The engine is the contribution, not the reader.** 98.1% on full LoCoMo says it surfaces the right evidence. We do not launder a retrieval win into a generation claim.

**In production.** Beyond benchmarks, FluctlightDB backs a continuously running operational agent in a deployed production system — persisting cross-session state for an autonomous service, not a research fixture. The durability and cold-start guarantees above are properties we rely on, not only ones we measured.

**Hybrid retrieval matters.** Lexical seeds raised LoCoMo recall over vector-only at moderate k — direct evidence the memory model benefits from machinery a pure vector DB lacks.

**Limitations.** CPU-heavy LongMemEval ingest; no multi-tenant-at-scale eval yet; no full LLM QA vs Mem0/Zep yet. Engineering and quota problems, not model problems.

## 6. Related Work

**Vector databases** (Chroma, Qdrant, FAISS) optimize similarity, not memory — no native episode, provenance-weighted recall, or cue-driven activation.

**Agent memory layers** sit above general backends:

| System | Kind | Native contract | LoCoMo (cited) | Metric |
|--------|------|-----------------|----------------|--------|
| Mem0 / Mem0^g | SDK + graph | Extract / consolidate / retrieve | ~92%+ LLM-J | End-to-end QA |
| Zep | Managed layer | Temporal KG + summaries | ~75% LLM-J | End-to-end QA |
| Cognee | Pipeline | Graph + vector ETL | — | Task-specific |
| MemGPT / Letta | Agent OS | Context tiers / blocks | — | Session QA |
| HippoRAG | Graph RAG | Associative retrieval | — | Multi-hop QA |
| **FluctlightDB** | **Engine** | **`experience()` / `activate()`** | **98.1%** | **Evidence recall** |

Mem0 (arXiv:2504.19413) is the primary reference to cite and differentiate against. Its graph variant and hybrid retrieval overlap in *mechanism* but not in *layer*: Mem0 orchestrates memory over backends; FluctlightDB defines memory as the store contract itself — a third data model peer to rows and vectors.

**Brain-native primitives:**

| Primitive | Role | Relational / vector analogue |
|-----------|------|------------------------------|
| Engram | Content + context + salience + provenance + edges | Row or chunk |
| `experience()` | Separation, encode, index, wire graph | INSERT / upsert |
| `activate(cue)` | Lexical + semantic seed, spread, fuse, trust boost | SELECT / ANN |
| Consolidation | Offline replay + `checkpoint()` | Vacuum only |
| Provenance | Verified sources outrank chat | No native type |

To our knowledge, no prior work positions long-term agent memory as a third data model with engine-level write/read semantics in an embedded store. Direct head-to-head LoCoMo *evidence recall* vs Mem0/Zep is future work (their published LoCoMo numbers use reader-LLM QA, not the same metric).

## 7. Conclusion

The relational model gave applications a database for *facts*; the vector model gave search a database for *similarity*. Autonomous agents need a database for *memory*, and it should be as boring to adopt and as rigorous to trust as SQLite. FluctlightDB is our argument that this engine can exist today: it matches vector baselines where they are strong, wins where memory semantics matter, and recalls 98.1% of gold evidence on the hardest public long-conversation benchmark.

## Artifacts

Repository: FluctlightDB (MIT). Harnesses: `locomo_eval.py`, `longmemeval_bench.py`, `beir_bench.py`, `agent_memory_bench.py`. Frozen metrics: `benchmarks/results/2025-06-22.json`.
