# Brain Figure Redesign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace overlapping Figure 1 with arXiv-grade dual-layer diagram (hero graph + technical panels).

**Architecture:** `matplotlib` + `networkx` in `papers/figures/generate_all.py`; `GridSpec` isolates panels; standalone hero export for README.

**Tech Stack:** Python 3, matplotlib, networkx, pdflatex

**Spec:** [`docs/superpowers/specs/2026-07-03-brain-figure-redesign-design.md`](../specs/2026-07-03-brain-figure-redesign-design.md)

---

### Task 1: Rewrite figure generator

**Files:**
- Modify: `papers/figures/generate_all.py`

- [x] Add hero graph panel with brain ellipse and directed activation flow
- [x] Stack panels (b–d) in dedicated axes
- [x] Bottom row: write, read, third-model comparison
- [x] Export `01-brain-hero.pdf/png` standalone

### Task 2: Paper integration

**Files:**
- Modify: `papers/arxiv-v1/main.tex`
- Modify: `papers/public/files/main.tex`

- [x] Update Figure 1 caption for panels (a)–(g)
- [x] Rebuild `papers/arxiv-v1/main.pdf`

### Task 3: Docs and assets

**Files:**
- Modify: `papers/figures/README.md`
- Copy: `papers/public/assets/*.png`

- [x] Document hero asset and design spec link
- [x] Sync PNGs to public viewer assets

### Task 4: Verify

- [x] Run `python3 papers/figures/generate_all.py` — no overlap at 100% zoom
- [x] Run `cd papers/arxiv-v1 && bash build.sh` — PDF builds

### Task 5: Publish (manual)

- [ ] `git add` + commit + push to `main`
- [ ] Optional: add `01-brain-hero.png` to repo README hero
