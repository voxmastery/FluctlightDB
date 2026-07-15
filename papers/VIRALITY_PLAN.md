# FluctlightDB — arXiv v2 Launch & Virality Plan

Goal: turn the arXiv v2 release ("A Memory Model of Data for AI Agents" + the Recall Fabric)
into a durable wave of developer attention, GitHub stars, and citations — not a one-day spike.

## The hook

One sentence people repeat: **"A database whose native operations are `experience()` and
`activate()` — an on-disk brain for agents, with grid-cell addressing and theta-gamma parsing,
and a live 3D viewer that lets you watch it think."**

The three shareable artifacts, in priority order:

1. **The Living Brain viewer** — a WebGL connectome that streams a real brain. This is the
   scroll-stopper. A 20-second screen recording of a recall wave lighting up engrams is the
   single most viral asset. Ship it first, everywhere.
2. **The Recall Fabric** — nine brain-native mechanisms (photon/lattice/phase/relation/
   crystallize/forgetting/chronos/confidence/consensus), each with a crisp neuroscience anchor.
   This is the "serious systems" credibility layer for HN / researchers.
3. **The numbers** — 96.8% raw LoCoMo evidence recall @k=150 (MiniLM-384; 97.0% mpnet-768),
   no neighbor expansion, from the native Rust CHORUS stack; 97.6% LongMemEval session@8,
   reproducible from a script. This is the "it's real" proof for skeptics.
   (Do not headline the old 99% — it was a ±3 `expand_session_neighbors` scoring artifact,
   not the engine; a trivial BM25 baseline also scored ~99% under it.)

## Assets to prepare (before launch day)

- [ ] 20s screen capture of the viewer: connect → recall probe → engrams pulse. Loopable GIF + MP4.
- [ ] A single hero image: the 3D brain with the vitals HUD (already renderable from `/brain`).
- [ ] `papers/figures/`: architecture (exists) + the Recall Fabric pipeline (Fig. 2 in v2).
- [ ] A 60–90s narrated demo video: install → `experience()` → `activate()` → open viewer.
- [ ] A "try it in 30 seconds" snippet that works copy-paste (pip install + 4 lines + `serve`).
- [ ] arXiv abstract trimmed to a tweet-length TL;DR pinned at top of README.

## Channels & sequencing

**Day 0 (Tue or Wed, ~14:00 UTC):**
- arXiv v2 goes live (cs.DB + cs.AI cross-list).
- Show HN: *"FluctlightDB – an embedded brain for AI agents (with a live 3D memory viewer)"*.
  Lead the post with the viewer GIF, then the one-liner, then the repro command. Answer every
  comment in the first 3 hours.
- X/Twitter thread (8–10 posts): hook GIF → problem (agents forget) → the third data model →
  the Fabric mechanisms one per post with the neuroscience anchor → numbers → repo link.

**Day 1–3:**
- Post to r/MachineLearning ("[R] ..."), r/LocalLLaMA, r/rust (angle: "a brain-native engine in
  pure Rust, no ML deps"), Lobsters.
- Submit the viewer to WebGL/three.js showcases and "awesome-*" lists (agents, RAG, memory).
- DM/❤️ the authors of Mem0, Zep, MemGPT, HippoRAG — invite comparison, not confrontation.

**Week 1–2:**
- Dev.to / personal blog long-form: *"Why agent memory is a third data model"* (the manifesto,
  expanded). Cross-post to Hashnode.
- Short YouTube / Loom: build a memory-persistent coding agent in 10 minutes using the repo.
- Reach newsletters: Latent Space, Ben's Bites, TLDR AI, Rundown, Import AI. Give them the GIF.

## Messaging guardrails (protect credibility)

- **Never conflate retrieval % with end-to-end QA %.** Always name the metric. This is the
  fastest way to lose researcher trust and the easiest attack for competitors.
- E2E LongMemEval is **still being measured** with the composed Fabric on the hot path — say so
  plainly. "Reproducible retrieval numbers today; E2E in progress via the open harness" beats an
  unverifiable claim.
- The Fabric mechanisms are **validated on synthetic property tests**, not yet proven to move E2E.
  Frame them as "foundations we can now measure," not "the reason we win."
- "Brain-native" is an architecture claim, not an AGI claim. Keep the AGI framing aspirational and
  clearly separated from measured results.

## Conversion (attention → usage → retention)

- README top: install + 4-line example + one `serve` command that opens the viewer. Zero friction.
- `fluctlight-project onboard` and the demo brain in the viewer mean a visitor sees value with
  **no server and no data** of their own.
- A "good first issue" set and CONTRIBUTING pointer so drive-by stars can become PRs.
- Pin the arXiv DOI + Zenodo DOI so citations accrue to a stable record.

## Success metrics

| Horizon | Metric | Target |
|---------|--------|--------|
| Launch day | HN front page | top 10 |
| Week 1 | GitHub stars | +1k |
| Week 1 | Viewer demo views | 50k |
| Month 1 | PyPI installs | 10k |
| Quarter | arXiv citations / mentions | first 5 |
| Quarter | External contributors | 5+ |

## What would make it *actually* spread

Virality follows a felt "whoa." For this project that whoa is **watching a memory form and be
recalled in 3D**. Everything else (numbers, mechanisms, the paper) converts the people the viewer
brings in. So: polish the viewer, record the clip, and lead with it in every channel.
