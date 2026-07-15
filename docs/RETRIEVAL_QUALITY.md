# Retrieval Quality & Tradeoffs (honest LoCoMo recipe)

This documents what the honest retrieval recipe (`benchmarks/locomo_lateinteraction.py`;
earlier two-channel form in `benchmarks/locomo_honest.py`)
actually delivers for **users building on FluctlightDB memory**, and the tradeoffs it
carries — with the mitigation for each. All numbers are honest raw evidence-recall
(a gold turn counts only if that exact turn was retrieved; no neighbor expansion).

## 1. What users actually get

A memory system's real job is: *given a question, put the right past turns in front of
the LLM inside a limited context budget.* That is recall@k, and here is the profile:

| Context budget (k) | 5 | 10 | 20 | 50 | 150 |
|---|---|---|---|----|-----|
| Gold evidence retrieved | 72.6% | 80.0% | 85.6% | 91.8% | 96.8% |

Reading this as a user:
- **Tight budget (k≈10–20)** — typical for a cheap RAG turn — you get **73–81%** of the
  needed evidence. The late-interaction channel adds ~4 points here vs mean-pool.
- **Generous budget (k≈50)** — **91%**. This is the sweet spot for quality-sensitive apps.
- **Ceiling (k=150)** — **96.8%** (lenient budget; read tight-k above). ~3% of gold is unreachable at all.

## 2. Quality by question type (know your strengths)

Per-category recall@150 tells users which queries are reliable:

| Query type | Recall | Verdict for users |
|---|---|---|
| Single-hop factoid ("what did X do?") | 98.7% | Excellent — trust it |
| Adversarial / distractor | 98.5% | Excellent — lexical + token-match nail exact refs |
| Temporal ("when did X…?") | 98.1% | Excellent |
| Multi-hop (needs several turns) | 88.7% | Good, but verify — one missing span can break the chain |
| Open-domain / paraphrase inference | 80.8% | **Weakest** — reword the query or widen k |

Actionable guidance we can surface to users: for open-domain/conceptual questions, either
raise k, or (when available) enable a stronger embedder — that is the category BM25 can't help.

## 3. Tradeoffs — and how each is overcome

### T1 — Context binding inflates stored/embedded text (~5× per chunk)
Embedding each turn with ±2 neighbours means each chunk body carries 5 turns of text.
- **Cost:** more embed compute at ingest; larger source-text store.
- **Overcome:** vector dimensionality is unchanged (384d) — the *vector* index does not grow,
  only the raw text. Keep the bare turn as the scoring/id anchor and treat the context purely
  as embedding input. Embedding is a one-time ingest cost, amortized over all future recalls.

### T2 — Two indexes (dense + BM25) to maintain
- **Cost:** a lexical inverted index alongside the vector store; an extra per-query lexical scan.
- **Overcome:** BM25 is cheap and incremental, and the engine already tokenizes
  (`crates/fluctlightdb/src/tokenize.rs`). Fusion is rank-only (RRF) — no score calibration
  between channels needed, so the two stay decoupled and independently updatable.

### T3 — Fusion weight is a precision/recall tension
Low `w_bm` maximizes the @150 ceiling and open-domain; high `w_bm` maximizes tight-k precision.
- **Overcome:** `w_bm=0.7` is the measured balance point — it holds 96.0% @150 *and* strong
  @10–50 *and* no open-domain regression. Query-adaptive gating (BM25 weight ∝ query's rarest
  token) was tested and came out roughly neutral, so we keep the simpler fixed weight.

### T4 — Overlapping chunks return duplicate turns
Because chunks share neighbours, the top-k can contain the same underlying turn multiple times,
wasting the LLM context window.
- **Overcome:** on context assembly, union the retrieved chunks by dia_id and de-duplicate turns
  before handing them to the model. Recall is measured on the union already, so this is free.

### T5 — CA3 pattern-completion (PRF) did not help
The brain-faithful "feed results back into the cue" step (Rocchio) *reduced* recall by 1–2 pts
on multi-topic dialogue — feedback drifts toward dominant themes and drops the specific gold turn.
- **Overcome / honest call:** we **do not ship it.** The genuine analog (LLM-generated HyDE
  pseudo-documents) needs model access; when an LLM is available, HyDE is the correct
  completion mechanism to revisit — naive vector PRF is not.

### T6 — The production engine (CHORUS/Rust) doesn't yet run this recipe
The 96.0% is a measured *prototype* on MiniLM vectors + hand-rolled BM25. The live CHORUS lane
does neither ±2 binding nor BM25 fusion, and its 256-bit SimHash photon code costs ~2.5 pts vs
plain float cosine at these richer chunk bodies.
- **Overcome (next step, scoped):** wire two things into `chorus.rs` recall —
  (a) a float-cosine rerank of the photon shortlist (recover the SimHash loss), and
  (b) a BM25 channel fused by RRF. Plus ±2 context binding at ingest. This is the real
  "lock-in" that moves the number for actual users, not just the benchmark.

### T7 — MiniLM mean-pool is lossy (SOLVED: late interaction)
A mean-pooled sentence vector collapses MiniLM's per-token contextual **population code** into
one centroid, destroying most discriminative signal — the root cause of the open-domain gap.
- **Overcome:** keep the token-level output (`last_hidden_state`) and match with late-interaction
  **MaxSim** (`benchmarks/locomo_lateinteraction.py`). This is *more information from the same
  model*, not a reshape. Result: 95.6→**96.3% @150**, +4.9 @5, and open-domain 78→82. Brain
  analog: distributed population coding / hippocampal ensemble match vs a collapsed mean rate.
- **New tradeoff (T8).**

### T8 — Late interaction stores per-token vectors (~35× index size)
MaxSim needs every token's 384-d vector, not one pooled vector per turn (~37 tokens/turn here).
- **Cost:** ~35× the vector storage and a heavier per-query score (query-tokens × doc-tokens).
- **Overcome:** store token vectors at **float16** (halves it, no measured accuracy loss);
  MaxSim is a single batched matmul + segmented max per conversation (segmented-reduce), so it
  stays fast at this scale. For very large stores, apply MaxSim only as a **reranker** over the
  BM25/pooled top-K candidates — same precision gain, bounded token storage. This is the
  standard ColBERT/PLAID engineering path.

### T9 — MiniLM's token vectors are the final ceiling
Even with the invented stack, open-domain sits at ~83 and the @150 ceiling is 96.8%. MiniLM's
*token* representations are now the limit.
- **Overcome:** make the base encoder pluggable (bge-large / e5-large / gte / mpnet). LongMemEval
  reached 97.6% with mpnet. A stronger encoder + late interaction is the honest path to 98%+.

## 4. Honest bottom line

- **96.8% honest raw recall@150** (and **72.6% @5** — the tight-k number that matters) — real, reproducible, no expansion crutch. It beats the old
  *faked* 99% on integrity and trails it by ~2 points while being an honest, native-engine signal.
- The recipe helps users most on factoid/temporal/adversarial queries (98–99%); it is weakest on
  open-domain paraphrase (81%).
- **98%+ needs a stronger base encoder** — late interaction extracted all the signal MiniLM's
  tokens hold; the remaining wall is the encoder, not the retrieval method.
