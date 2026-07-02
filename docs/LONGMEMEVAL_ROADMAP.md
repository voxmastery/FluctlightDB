# LongMemEval 90%+ roadmap

Research → invent → test loop for FluctlightDB on [LongMemEval-S](https://github.com/xiaowu0162/LongMemEval) (ICLR 2025).

## Two different “90%” targets

| Metric | What it measures | SOTA | Fluctlight (turn, running) |
|--------|------------------|------|----------------------------|
| **Session recall@K** | Gold `answer_session_ids` in top-K | **95–98% @5** (gbrain, YourMemory) | **98% @8** on first 50 (user-only slice) |
| **Answer-in-recall@K** | Answer string tokens in recalled text | N/A (not official) | **~40% @8** (turn-level) |
| **End-to-end QA (LLJ)** | LLM reads retrieval + answers; GPT judge | **76–90%** (TiMem, PlugMem) | Not run yet |

**Preference questions** (30/500) have *meta* answers (“user would prefer…”) that never appear verbatim in chat — answer-in-recall cannot reach 90% without an LLM reader. **Session recall** is the fair retrieval bar.

## Root cause of ~40% turn-level score

1. **Wrong granularity** — one engram per *turn* vs LongMemEval *session* as the retrieval unit.
2. **480-char truncation** — cuts evidence mid-session.
3. **Metric mismatch** — substring match on `answer` vs official `answer_session_ids`.
4. **No key expansion** — paper uses user-fact / keyphrase keys (CP2).

## What we implemented (2026-07-02)

```bash
python3 benchmarks/longmemeval_bench.py \
  --granularity session \
  --metric session \
  --mode index \
  --top-k 8
```

- One engram per `haystack_sessions[i]` with `doc_id=haystack_session_ids[i]`.
- User utterances prepended as FTS keys (LongMemEval CP2 heuristic).
- Session date in content prefix for temporal FTS.
- Full session body up to 12k chars (no 480 cut).
- `session_recall_at_k` metric aligned with leaderboard.

## Research synthesis (what beats what)

### LongMemEval paper (Wu et al., ICLR 2025)

- **CP1 Value**: session decomposition > whole-history.
- **CP2 Key**: multi-key indexing (summaries, keyphrases, user facts) → +5–10% recall.
- **CP3 Query**: time-aware range filter on indexed event dates → +6–11% on temporal.
- **Retriever**: hybrid BM25 + dense; user-only keys for indexing.

### Leaderboard systems (retrieval)

| System | R@5 | Techniques |
|--------|-----|------------|
| gbrain | 97.6% | hybrid + query expansion; text-embedding-3-large |
| YourMemory | 95.8% | multi-qa-mpnet + BM25 + graph BFS |
| agentmemory | 95.2% | BM25+vector, all-MiniLM-L6-v2 |
| M3 Memory | 96.8% R@10 | FTS5 + vector + MMR |

### Agent-memory architectures (QA, not pure retrieval)

| System | LongMemEval-S QA | Idea |
|--------|------------------|------|
| TiMem | 76.9% LLJ | Temporal memory tree, hierarchical consolidation |
| PlugMem | 90.2% Acc | Graph memory + task adaptation |
| SYNAPSE | SOTA LoCoMo | Spreading activation on capped subgraph |
| SwiftMem | 11 ms search | Temporal index + DAG-tag routing |

### Fluctlight mapping

| Research idea | Fluctlight hook |
|---------------|-----------------|
| Session value unit | `--granularity session` |
| Hybrid lexical+dense | `recall_index` FTS5 + HNSW |
| Capped subgraph | `FLUCTLIGHT_CANDIDATE_CAP` |
| Spreading activation | `connect()` / `connect_agent_fast()` |
| No LLM on read | `activate()` — already true |
| Time-aware filter | **TODO**: `question_date` + `haystack_dates` pre-filter |
| LLM key expansion | **TODO**: optional fact extraction on ingest |
| Better embeddings | `FLUCTLIGHT_EMBED_MODEL=multi-qa-mpnet-base-dot-v1` |

## Test loop (ongoing)

```
research papers / leaderboards
        ↓
hypothesis (granularity, keys, temporal, embed model)
        ↓
benchmarks/longmemeval_bench.py --limit N  (fast)
        ↓
full 500 + by_type breakdown
        ↓
if < 90% session_recall@8 → next hypothesis
```

### Next experiments (priority)

1. **Full 500 session_recall@8** — confirm ≥90% on all types.
2. **Skip abstention (470 Q)** — match official retrieval eval.
3. **Embedding model** — `multi-qa-mpnet-base-dot-v1` (retrieval-tuned Q→passage).
4. **Temporal pre-filter** — parse `question_date` / relative time → restrict candidate sessions.
5. **RRF fusion** — explicit BM25+dense rank merge in sidecar (if lexical+dense gap shows in by_type).
6. **E2E QA** — retrieve top-8 sessions → GPT-4o-mini answer → official LLJ (TiMem comparison).

## Commands

```bash
# Official-style retrieval (target ≥90%)
PYTHONPATH=sdks/python python3 benchmarks/longmemeval_bench.py \
  --granularity session --metric session --mode index --top-k 8

# Legacy turn / answer-string metric (expect ~40%)
PYTHONPATH=sdks/python python3 benchmarks/longmemeval_bench.py \
  --granularity turn --metric answer --mode index --top-k 8

# Faster embed model swap
export FLUCTLIGHT_EMBED_MODEL=multi-qa-mpnet-base-dot-v1
```

## References

- Wu et al., *LongMemEval*, ICLR 2025. [PDF](https://openreview.net/pdf?id=d813d324dbf0598bbdc9c8e79740ed01)
- Jiang et al., *SYNAPSE*, ACL Findings 2026.
- Tian et al., *SwiftMem*, arXiv:2601.08160.
- TiMem, ACL Findings 2026 (76.88% LongMemEval-S LLJ).
- gbrain evals: 97.6% R@5 hybrid on cleaned LongMemEval-S.
