# LongMemEval world-record trophy — design

**Date:** 2026-07-13  
**Status:** Approved for planning (brainstorm)  
**Repo:** FluctlightDB  
**Principle:** Earn the moat in the harness. Do not demote or rewrite the @8 ceiling story until a stricter, unique win is real.

## Goal

Be **best on LongMemEval-S retrieval** for paper, GitHub outsiders, and product — one package that BM25 cannot also claim, and that beats the published record.

## Non-goals

- Paper/README claim rewrites before freeze numbers exist
- Selling **97.6% session_recall@8** as a unique engine moat (BM25 also reaches 488/500)
- LoCoMo, E2E judge QA, or Fabric-only score paths that are not default `activate`
- Expanding or redefining the hard-slice set after freeze (cherry-picking)

## Trophy package (success = all three)

| Pillar | Target | Why |
|--------|--------|-----|
| **Official record** | **≥97.7% session_recall@5** on full cleaned LongMemEval-S (500) | Beats gbrain **97.6% @5**; comparable leaderboard language |
| **Hard-slice moat (B)** | On frozen \(H\) = questions where **BM25@8 misses**, Fluctlight@8 (and ideally @5) **beats BM25** by a clear margin | Proves hybrid/dense is not theater when lexical fails |
| **Typed record (C)** | Preference **30/30** session_recall@8 (also report @5) | Real gap vs lexical (~86.7% @8 on preference); closes current 29/30 |

**Ceiling (kept, not sold as moat):** Full-500 session_recall@8 ≈ **97.6%** remains the saturated reproducibility number. Always publish a **BM25 column** beside it once the baseline freeze exists.

### Claim gates (no public “#1” until true)

| Claim | Gate |
|-------|------|
| Hard-slice win | Us − BM25 ≥ **+1 absolute hit** on frozen \(H\); aim ≥ **+10pp** if \|H\| ≥ 10 |
| Preference trophy | **30/30** on same v4 preference protocol (dual-key, pref-facts, query-expand) |
| World #1 @5 | Fluctlight@5 **> 97.6%** on full 500, same metric as published peers |
| Aggregate @8 | May stay in docs; **must sit next to BM25** after freeze |

## Context (current reality)

- Freeze: Colab v4 mpnet, **488/500 = 97.6% @8**; preference **29/30** (miss `95228167`); temporal weakest typed bucket (~95.5% @8).
- Outsider: plain **BM25 also 488/500 @8** — aggregate @8 is saturated.
- Preference: mpnet path **96.7%** vs lexical **~86.7%** (n=30) — real but not enough alone for world #1.
- Peer record to beat: **gbrain 97.6% @5** (hybrid + text-embedding-3-large); YourMemory 95.8% @5.
- Do **not** compare our @8 to their @5.

## Approach chosen

**Hard-slice scoreboard + last-miss preference hunt**, then @5 record hunt — not rerank-only theater, not preference-only moonshot.

## Architecture

### Measurement spine (ship first)

One freeze runner (extend `benchmarks/longmemeval_bench.py` or a thin sibling) that, **per question**, records ranked session IDs for:

1. **BM25-only** (no dense / no embedder)
2. **Fluctlight default v4** (mpnet + hybrid + dual-key + pref-facts + query-expand)

Emit metrics at **K ∈ {1, 3, 5, 8}**, plus:

- Aggregate table (us vs BM25)
- `by_type` breakdown
- Frozen **\(H\)** = `{question_id | BM25 session_recall@8 is miss}` with locked ID list
- Preference subset scores and miss IDs
- Full miss lists for Fluctlight@5 and @8

Until this spine exists, “world #1” is unverifiable.

### Engineering loop (strict order)

1. **Baseline freeze** — BM25 vs current v4 @1/3/5/8; lock \(H\) and preference miss `95228167`.
2. **Preference → 30/30** — autopsy that miss; fix keys / pref-facts / multi-query / ranking on the preference path only as needed; re-run preference slice, then full 500.
3. **@5 record hunt** — reach **≥489/500 @5**. Lever order (honesty first):
   - Better **keys** (CP2-style) on miss types (especially temporal)
   - **Time-aware filter** (`question_date` + haystack dates) — already on roadmap
   - Hybrid / shortlist fusion weights
   - Stronger embedder **only if** same-model ablation still shows an engine win (document embedder column)
4. **Hard-slice maximization** — every change reports Δ on \(H\); reject changes that only move easy questions.
5. **Record freeze** — publish JSON + harness flags + commit SHA: Fluctlight@5, BM25@5/@8, \|H\|, pref 30/30.

### Components

| Unit | Responsibility | Depends on |
|------|----------------|------------|
| Dual-system recall harness | Run BM25-only and Fluctlight v4 per question; multi-k scoring | Existing `longmemeval_bench.py`, session metric |
| Hard-slice freeze artifact | Immutable list of BM25@8 miss IDs + generation metadata | Dual-system harness |
| Preference autopsy path | Isolate miss `95228167`; iterate keys/queries without full-500 cost | Preference profile flags |
| Record freeze JSON | Single source of truth for docs / issue #2 / paper tables | Harness outputs |
| Ablation logger | Per-change: Δ@5, Δ@8, Δ\(H\), Δpref | Freeze artifacts |

### Data flow

```
LongMemEval-S cleaned JSON
        ↓
per-question ingest (session granularity, v4 keys)
        ↓
┌─────────────────┬──────────────────────┐
│ BM25-only recall│ Fluctlight v4 recall │
└────────┬────────┴──────────┬───────────┘
         ↓                   ↓
    ranked session ids (K=1,3,5,8)
         ↓
  aggregate + by_type + H + preference
         ↓
  freeze JSON → claim gates → public #1 only if all pass
```

### Error handling / integrity

- If BM25 and Fluctlight share an ingest bug, both inflate — validate a sample of gold `answer_session_ids` membership independently.
- If \|H\| is empty at @8, redefine hard-slice as **BM25@5 misses** (document the switch once; do not flip again).
- Prefer failing the claim gate over soft scoring (no neighbor expansion; LongMemEval stays official session_recall).

### Testing

- Unit: session_recall@K helper for multiple K; hard-slice set membership frozen.
- Integration: small `--limit` dual run produces BM25 column + \(H\).
- Full: Colab/GPU full 500 for record freeze; preference-only loop for 30/30.
- Regression: after each lever, refuse merge of “win” docs without freeze JSON update.

## Out of scope (deferred)

- LoCoMo raw uplift (separate trophy)
- E2E LLJ QA race with TiMem/PlugMem
- Shared-brain provenance narrative (honesty, not this retrieval trophy)
- Abstract rewrite demoting 97.6%@8 before @5 + \(H\) + pref gates pass

## Success checklist

- [ ] Dual BM25 vs Fluctlight freeze at K=1,3,5,8
- [ ] \(H\) locked and published
- [ ] Preference 30/30
- [ ] Fluctlight session_recall@5 ≥ 97.7% (≥489/500)
- [ ] Hard-slice gate met
- [ ] Record freeze JSON + reproduce command in `docs/` / benchmarks README
- [ ] Only then: public #1 / issue #2 / paper table update

## Risks

| Risk | Mitigation |
|------|------------|
| @5 already saturated for everyone | Hard-slice + preference still prove moat; report @1/@3 |
| Proprietary embedder “buys” @5 | Always show BM25 + same-embedder ablation |
| Tiny \|H\| | Prefer BM25@5 misses; require absolute hit gains |
| Preference n=30 overfit | Keep full-500 @5 as primary record; pref is typed trophy |

## Decision log

- Audience: paper + GitHub + product (D)
- Moat substance: hard-slice (B) + preference 30/30 (C); @5 for world record
- No lead-claim rewrite until reality exists
- Approach: measurement spine → pref last miss → @5 → maximize \(H\) → freeze
