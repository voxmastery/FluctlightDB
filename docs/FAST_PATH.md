# Fast path — embedded recall for agents

FluctlightDB can beat vector-DB latency on the **read path** when you use embedded mode and the hybrid recall index. This page maps research-backed techniques to concrete APIs and env vars.

## Decision tree

```
Need memory in your agent loop?
│
├─ Same process as the agent (default)
│   └─ connect_agent_fast(path)  or  get_recall_client(path)
│       • ~0.05–2 ms/query on warm brains with sidecar index
│       • Full write path (provenance, dentate, graph)
│
├─ Bulk RAG / IR benchmark only
│   └─ connect_index(path)  — vector-fast (0 graph hops)
│
├─ Long conversational eval (LoCoMo / LongMemEval)
│   └─ connect_conv(path)  — fast ingest + hybrid recall
│
└─ Remote / multi-tenant / ops-owned brain
    └─ FluctlightClient HTTP
        • activate-lite (top-1, ~200 B)
        • activate-batch (many cues / round-trip)
        • Prefer embedded recall replica for hot path (see DEPLOYMENT.md)
```

## Research → Fluctlight mapping

Recent agent-memory systems converge on the same pattern: **pre-filter candidates cheaply, then score a small subgraph — never scan full history on every turn.**

| System | Core idea | Fluctlight equivalent |
|--------|-----------|----------------------|
| **[SYNAPSE](https://aclanthology.org/2026.findings-acl.1108/)** (ACL 2026) | Triple hybrid: embeddings + spreading activation on a **capped subgraph**; ~1.9s latency vs 8s+ full context | `recall_index` FTS5+HNSW seeds → `activate_from_hybrid` with `FLUCTLIGHT_CANDIDATE_CAP`; `connect_agent_fast` limits spread to 1 hop |
| **[SwiftMem](https://arxiv.org/abs/2601.08160)** (2026) | Query-aware temporal + semantic DAG-tag index; **~11 ms** search, 47× faster than Zep/Nemori on LoCoMo | Sidecar hybrid index + temporal fields on episodes; tag routing on roadmap |
| **[Zep / Graphiti](https://www.getzep.com/)** | Graph + vector + BM25 fusion; **no LLM on read path** | Hybrid lexical+semantic candidates; activation cache; no LLM in `activate()` |
| **[Mem0](https://mem0.ai/)** v3 | Parallel semantic + BM25 + entity fusion; async writes | `experience` async via HTTP; embedded writes + `checkpoint()` batching |
| **[Nemori](https://arxiv.org/abs/2410.18415)** | Episode-level integration + semantic distillation | `sleep()` / consolidation; provenance-weighted recall |
| **[LiCoMemory / CogniGraph](https://arxiv.org/abs/2502.06402)** | Lightweight graph as **index**, not heavy node payloads | Engrams hold content; graph stores wiring; sidecar holds search keys |
| **[Letta](https://docs.letta.com/)** | Hierarchical paging: core / recall / archival | `activate` (hot) vs `list` / export (cold); `activate-lite` for top-1 |
| **PRISM** (typed-path retrieval) | Route by memory type before scoring | `context`, `provenance_kind`, agent scoping in `activate_scoped` |

### What we implemented from this

1. **`connect_agent_fast()`** — sets `FLUCTLIGHT_AGENT_FAST=1`, `FLUCTLIGHT_CANDIDATE_CAP=96` (override via env). Recall uses sidecar hybrid seeds + **1-hop** graph spread (full agent write path unchanged).
2. **`connect_index()`** — sets `FLUCTLIGHT_VECTOR_FAST=1` → **0 hops** (pure hybrid scoring; Chroma-class speed for IR).
3. **`activation_cache`** — repeat cues avoid re-spread (Letta-style hot recall).
4. **`activate-lite` HTTP** — top-1 JSON (~200 bytes) for remote agents.
5. **`activate_batch`** — one lock, many cues (ServerBrain / multi-tool turns).

## Setup (agent fast path)

```python
from fluctlightdb import connect_agent_fast

brain = connect_agent_fast("/tmp/my-agent-brain")
brain.experience("User prefers dark mode", context="settings", salience=0.7)
print(brain.activate("dark mode"))
brain.checkpoint()
```

After bulk backfill or migration, rebuild the sidecar:

```bash
fluctlight index rebuild --path /tmp/my-agent-brain
```

Check index presence from Python: `brain.has_sidecar_index()`.

## Environment variables

| Variable | Default | Effect |
|----------|---------|--------|
| `FLUCTLIGHT_AGENT_FAST` | off | 1-hop spread + hybrid pre-filter (agent mode) |
| `FLUCTLIGHT_VECTOR_FAST` | off | 0-hop spread (index / vector-only mode) |
| `FLUCTLIGHT_CANDIDATE_CAP` | 128 (96 in agent-fast) | Max engrams scored per query (SYNAPSE-style subgraph cap) |
| `FLUCTLIGHT_FAST_INGEST` | off | Set by `connect_index` / `connect_conv` for bulk ingest |

## HTTP when you cannot embed

Embedded recall is always fastest. If the brain must live behind `fluctlight serve`:

```python
from fluctlightdb import FluctlightClient

client = FluctlightClient.from_env()
# Top-1, minimal JSON
lite = client.activate_lite("dark mode")
# Many cues per request
batch = client.activate_batch(["dark mode", "timezone"])
```

For production agents at scale, run a **read-only embedded replica** for recall and HTTP for writes (see [DEPLOYMENT.md](DEPLOYMENT.md)).

## Expected latency (order of magnitude)

| Path | Typical p50 | Notes |
|------|-------------|-------|
| Embedded + sidecar, fresh brain | **0.05–0.2 ms** | Measured on empty/small brains |
| Embedded, 10k engrams + index | **1–5 ms** | `FLUCTLIGHT_SCALE_BENCH=1` bar |
| `connect_index` / vector-fast | **~Chroma ANN** | 0 graph hops |
| HTTP localhost, small brain | **1–50 ms** | keep-alive + lite |
| HTTP prod, 40k+ synapses | **seconds** | use embedded recall or replica |

## References

- Jiang et al., *SYNAPSE: Empowering LLM Agents with Episodic-Semantic Memory via Spreading Activation*, ACL Findings 2026. [PDF](https://aclanthology.org/2026.findings-acl.1108.pdf)
- Tian et al., *SwiftMem: Fast Agentic Memory via Query-aware Indexing*, arXiv:2601.08160, 2026.
- Zep / Graphiti temporal knowledge graph documentation.
- Mem0 hybrid retrieval and async write pipeline (2025–2026).
- Li et al., *LiCoMemory: Lightweight Cognitive Graph for Long-Term Memory*, 2025.
