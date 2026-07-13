# Retrieval Quality & Tradeoffs (honest LoCoMo recipe)

This documents what the honest two-channel retrieval recipe (`benchmarks/locomo_honest.py`)
actually delivers for **users building on FluctlightDB memory**, and the tradeoffs it
carries — with the mitigation for each. All numbers are honest raw evidence-recall
(a gold turn counts only if that exact turn was retrieved; no neighbor expansion).

## 1. What users actually get

A memory system's real job is: *given a question, put the right past turns in front of
the LLM inside a limited context budget.* That is recall@k, and here is the profile:

| Context budget (k) | 5 | 10 | 20 | 50 | 150 |
|---|---|---|---|----|-----|
| Gold evidence retrieved | 60.9% | 71.9% | 80.2% | 90.1% | 96.0% |

Reading this as a user:
- **Tight budget (k≈10–20)** — typical for a cheap RAG turn — you get **72–80%** of the
  needed evidence. Good, not perfect.
- **Generous budget (k≈50)** — **90%**. This is the sweet spot for quality-sensitive apps.
- **Ceiling (k=150)** — **96.0%**. Only 4% of gold is unreachable at all.

## 2. Quality by question type (know your strengths)

Per-category recall@150 tells users which queries are reliable:

| Query type | Recall | Verdict for users |
|---|---|---|
| Single-hop factoid ("what did X do?") | 99.0% | Excellent — trust it |
| Adversarial / distractor | 97.9% | Excellent — the lexical channel nails exact refs |
| Temporal ("when did X…?") | 97.1% | Excellent |
| Multi-hop (needs several turns) | 88.1% | Good, but verify — one missing span can break the chain |
| Open-domain / paraphrase inference | 79.3% | **Weakest** — reword the query or widen k |

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

### T7 — MiniLM is the semantic ceiling
Open-domain 79.3% is capped by a weak 2021 384-dim embedder; no amount of chunking/fusion
fixes paraphrase.
- **Overcome:** make the embedder pluggable (bge-large / e5-large / gte / mpnet). LongMemEval
  already reached 97.6% with mpnet. This is the single biggest remaining lever and the only
  honest path from 96% → 98%+.

## 4. Honest bottom line

- **96.0% honest raw recall@150** — real, reproducible, no expansion crutch. It beats the old
  *faked* 99% on integrity and trails it by only 4 points while being an actual engine signal.
- The recipe helps users most on factoid/temporal/adversarial queries (97–99%); it is weakest on
  open-domain paraphrase (79%).
- **98%+ is not reachable with the current embedder** — that is the honest wall, and the fix is
  a stronger embedder, not more retrieval tricks.
