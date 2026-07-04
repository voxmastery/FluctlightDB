# LongMemEval 90%+ roadmap

Research → invent → test loop for FluctlightDB on [LongMemEval-S](https://github.com/xiaowu0162/LongMemEval) (ICLR 2025).

## Two different “90%” targets

| Metric | What it measures | SOTA | Fluctlight (session@8) |
|--------|------------------|------|------------------------|
| **Session recall@K** | Gold `answer_session_ids` in top-K | **95–98% @5** (gbrain, YourMemory) | **97.6% @8** (unified v4, mpnet Colab) |
| **Answer-in-recall@K** | Answer string tokens in recalled text | N/A (not official) | Deprecated (turn-level was ~40%) |
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

**Status (2026-07-04):** Unified v4 full 500 on Colab GPU: **97.6%** (488/500) session@8. Preference **96.7%** (29/30) — **90% target met**.

### Next experiments (post-arXiv v1)

1. **Preference v4 Colab run** — `longmemeval_colab.ipynb` now defaults to `preference` profile + `PREF_FACTS_KEY` (mpnet GPU). Paste result → freeze JSON if ≥90%.
2. **End-to-end QA** — reader LLM + GPT judge vs Mem0/Zep ([LONGMEMEVAL_E2E.md](LONGMEMEVAL_E2E.md)).
3. **Temporal pre-filter** — `question_date` + `haystack_dates` candidate restriction.
4. **LLM key expansion** — optional fact extraction on ingest (LongMemEval CP2 full).

See also: `docs/LONGMEMEVAL_E2E.md` for retrieval vs end-to-end metric separation.

## Results snapshot (2026-07-03)

| Run | Metric | Score | Notes |
|-----|--------|-------|-------|
| v1 session (lexical-only embed bug) | session@8 | **93.8%** (469/500) | preference 53.3% |
| preference v2 (MiniLM + dual-key + expand) | session@8 | **73.3%** (22/30) | +20pp on preference |
| v2 fast (lexical + dual-key + expand) | session@8 | **~91%** | lexical-only baseline |
| **Colab GPU mpnet v2 unified 500** | session@8 | **97.6%** (488/500) | full v4 run, pref-facts + dual-key |
| ↳ knowledge-update | session@8 | **100%** | |
| ↳ multi-session | session@8 | **98.5%** | |
| ↳ single-session-user | session@8 | **97.1%** | |
| ↳ single-session-assistant | session@8 | **98.2%** | |
| ↳ temporal-reasoning | session@8 | **95.5%** | |
| ↳ single-session-preference | session@8 | **96.7%** (29/30) | v4 mpnet Colab 2026-07-04 |

Frozen result: `benchmarks/results/longmemeval-colab-mpnet-2026-07-03.json`

```bash
# Colab (recommended for full 500 + GPU embeds)
# benchmarks/longmemeval_colab.ipynb — multi-qa-mpnet, dual-key, query-expand
```

```bash
# Lexical v2 (dual-key + query-expand, no embed)
./scripts/longmemeval-v2-run.sh fast

# Full 500 with MiniLM embeds (:8793)
export FLUCTLIGHT_EMBED_URL=http://127.0.0.1:8793
./scripts/longmemeval-v2-run.sh full

# Full 500 with retrieval-tuned mpnet (:8794) — recommended
./scripts/start-embed-mpnet.sh   # or: ./scripts/longmemeval-v2-run.sh full-mpnet
export FLUCTLIGHT_EMBED_URL=http://127.0.0.1:8794

# Auto: wait for preference slice, then full-mpnet
./scripts/longmemeval-v2-run.sh watch
```

## References

- Wu et al., *LongMemEval*, ICLR 2025. [PDF](https://openreview.net/pdf?id=d813d324dbf0598bbdc9c8e79740ed01)
- Jiang et al., *SYNAPSE*, ACL Findings 2026.
- Tian et al., *SwiftMem*, arXiv:2601.08160.
- TiMem, ACL Findings 2026 (76.88% LongMemEval-S LLJ).
- gbrain evals: 97.6% R@5 hybrid on cleaned LongMemEval-S.
