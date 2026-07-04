# LongMemEval end-to-end QA (vs Mem0 / Zep)

Retrieval numbers in the paper (**98.0%** session@8 composite) and Mem0/Zep headline numbers (~92% / ~75% on LoCoMo) measure **different layers**. This doc explains the gap and how to run a fair comparison later.

## Two evaluation layers

| Layer | What it tests | FluctlightDB today | Mem0 / Zep cited |
|-------|----------------|-------------------|------------------|
| **Retrieval** | Gold session/evidence in top-K | **98.0%** LongMemEval session@8 (composite) | Often reported internally, not always public |
| **End-to-end QA** | Retrieve → LLM reads → answer → GPT judge | **Colab `v2` profile** | LoCoMo ~92% / ~75% LLM-J |

FluctlightDB's contribution is the **engine retrieval layer**. End-to-end QA additionally depends on:

1. Reader LLM (GPT-4, Claude, etc.)
2. Prompt / context budget
3. Answer extraction and judge model

A 99.5% retrieval slice with a reader LLM pilot scored **23.5% category F1** on LoCoMo — retrieval was not the bottleneck; generation was.

## Official LongMemEval QA metric

Wu et al. (ICLR 2025) report **LLM-as-judge** accuracy when the memory module feeds an answer model. Published baselines on LongMemEval-S:

| System | Metric | Score (approx.) |
|--------|--------|-----------------|
| TiMem | LLJ | ~77% |
| PlugMem | Accuracy | ~90% |
| Mem0 / Zep | LoCoMo LLM-J | ~92% / ~75% (different benchmark) |

**Do not** compare our 98.0% session recall to Mem0's ~92% without labeling retrieval vs end-to-end.

## Planned harness (post-arXiv)

**Implemented:** `benchmarks/longmemeval_e2e.py` — retrieval + official reader prompt + GPT-4o judge.

Colab: `BENCH_PROFILE = "v2"` (500 retrieval + e2e) or `"e2e"` only; set `OPENAI_API_KEY` in Secrets.

```bash
export OPENAI_API_KEY=...
export FLUCTLIGHT_EMBED_URL=http://127.0.0.1:8794
python3 benchmarks/longmemeval_e2e.py \
  --dual-key --pref-facts-key --query-expand \
  --limit 50 \
  --json-out benchmarks/results/longmemeval-e2e-v4-mpnet.json
```

## Minimum viable comparison vs Mem0/Zep

1. **Same benchmark split** — LongMemEval-S cleaned JSON (500 Q).
2. **Same reader LLM** — e.g. GPT-4o-mini or gpt-4o (document version).
3. **Same judge** — official LongMemEval judge prompt.
4. **Report both** — session_recall@K **and** end-to-end accuracy in one table.

## Cost estimate

- 500 questions × (~2k input + ~200 output tokens) × 2 calls (reader + judge) ≈ **$15–40** on GPT-4o-mini (prices vary).
- Run on a 50-question slice first (~$2).

## Preference slice — **96.7% (29/30)** ✓

Preference failures are **retrieval** failures: the question is generic ("documentary recommendations?") but the gold session mentions specific prior facts ("Our Planet", mixology class, etc.).

**v4 harness result (2026-07-04):** mpnet + dual-key + pref-facts-key + query-expand on Colab GPU → **29/30** session@8. Sole miss: `95228167`. Frozen: `benchmarks/results/longmemeval-preference-v4-mpnet-2026-07-04.json`.

**v4 harness** (in `longmemeval_bench.py`):

- `--pref-facts-key` — third engram with extracted user facts per session
- Domain query bridges (dinner, cocktail, documentary, commute, …)
- RRF merge for preference multi-query

**Colab (recommended):** [`longmemeval_colab_v2.ipynb`](../benchmarks/longmemeval_colab_v2.ipynb) — **not** the old `longmemeval_colab.ipynb` (Colab caches it).

Open: https://colab.research.google.com/github/voxmastery/FluctlightDB/blob/main/benchmarks/longmemeval_colab_v2.ipynb

Paper v2: `BENCH_PROFILE = "v2"`, `E2E_LIMIT = 500`, GPU runtime, **`OPENAI_API_KEY` in Secrets** (real OpenAI key — **not** Cursor `crsr_*` Agent SDK key).

Local CLI (needs mpnet embed sidecar):

```bash
export FLUCTLIGHT_EMBED_URL=http://127.0.0.1:8794   # mpnet
python3 benchmarks/longmemeval_bench.py \
  --granularity session --metric session \
  --dual-key --pref-facts-key --query-expand \
  --type-filter single-session-preference \
  --top-k 8 --mode index \
  --json-out benchmarks/results/longmemeval-preference-v4-mpnet.json
```

**Baselines:** lexical v4 = **86.7%** (26/30); mpnet without v4 = **76.7%** (23/30). **Achieved: 96.7%** mpnet + v4 (29/30).

## arXiv positioning

Submit now with:

- Strong retrieval evidence (LoCoMo + LongMemEval 98% composite)
- Honest limitation: e2e QA deferred; composite 500 from full + preference slice
- This doc + roadmap for post-preprint work

Upgrade preprint on arXiv v2 when full 500 v4 run confirms composite and/or e2e table is complete.
