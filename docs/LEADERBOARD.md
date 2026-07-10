# Leaderboard & public results policy

FluctlightDB **does not** publish headline benchmark numbers to a third-party agent-memory leaderboard today. This page explains why, what we do instead, and what would change that.

## Agent-memory “leaderboards” (LoCoMo, LongMemEval)

| Fact | Detail |
|------|--------|
| **Canonical live leaderboard?** | **No** — LoCoMo and LongMemEval have official datasets/papers, but no neutral site where vendors submit scores and get ranked like BEIR or MTEB |
| **What Mem0/Zep/etc. publish** | Mostly **self-reported** blog posts, PDF tables, or GitHub README percentages |
| **What we publish** | Frozen JSON in-repo + open reproduce scripts + honest [REPRODUCIBILITY.md](REPRODUCIBILITY.md) |
| **Third-party submission status** | **Not submitted** anywhere external for LoCoMo / LongMemEval |

Our public numbers live here:

| Artifact | Purpose |
|----------|---------|
| `benchmarks/results/paper-2026-07-09.json` | Paper freeze bundle |
| `benchmarks/results/locomo-chorus-fabric-2026-07-09.json` | LoCoMo cert (99.0% evidence recall, Fabric on) |
| `make reproduce-locomo` | Anyone can verify LoCoMo cert locally |
| GitHub Releases + PyPI | Versioned engine, not a ranking |

## What we are **not** doing yet (on purpose)

| Action | Status | Why |
|--------|--------|-----|
| Mem0/Zep head-to-head blog post | **Blocked** | Credible only after **first external** LoCoMo reproduction ([REPRODUCIBILITY.md](REPRODUCIBILITY.md)) |
| Vendor roundup outreach (EverMind-style) | **Not started** | Waiting on independent repro of headline numbers |
| Claiming “#1 on LoCoMo leaderboard” | **Never** | That leaderboard does not exist as a neutral registry |

## What we **can** submit (optional, different metrics)

| Venue | Metric type | Fit |
|-------|-------------|-----|
| [BEIR leaderboard](https://github.com/beir-cellar/beir) | IR nDCG@10 | SciFact 0.645 — standard IR, not agent E2E QA |
| arXiv / Zenodo | Paper + frozen JSON | Already done (DOI on README) |
| Hugging Face dataset card | Reproduce scripts | Optional future |
| Independent repro issue | “Verified by X” | **Preferred** — public credit in REPRODUCIBILITY.md |

## When we **will** pursue external visibility

1. **First external LoCoMo match** → link from README + optional comparison post on identical protocol  
2. **Independent LoCoMo/LongMemEval repro** → add to REPRODUCIBILITY table  
3. **Co-maintainer or named reviewer** → stronger bus-factor story for adoption posts  

## Summary

> **We are not withholding from a leaderboard — there is no neutral agent-memory leaderboard to submit to.**  
> We publish **reproducible artifacts** instead of registry entries. External credibility comes from independent reproduction, not from us self-listing on a vendor comparison site.

See also: [MAINTAINER.md](../MAINTAINER.md) · [BENCHMARKS.md](BENCHMARKS.md)
