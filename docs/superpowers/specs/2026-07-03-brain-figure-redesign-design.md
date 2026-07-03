# Brain Architecture Figure Redesign — Design Spec

**Date:** 2026-07-03  
**Status:** Approved (user: “go with what you recommend”)  
**Scope:** Replace overlapping `01-brain-architecture` with arXiv-grade dual-panel figure.

## Problem

Current Figure 1 (`papers/figures/generate_all.py`) places a tall “Third data model” box at x=0.64 overlapping panel (b) Engram. Text clips (`unit) d data model`). Not suitable for arXiv submission or GitHub hero use.

## Approach (Option C — approved)

One downloadable asset for the paper, two visual layers inside it:

| Panel | Role | Audience |
|-------|------|----------|
| **(a) Hero** | Cue-driven activation graph inside a subtle brain contour; Connected Papers / GNN aesthetic | GitHub README, landing, social |
| **(b–f) Technical** | Fixed-layout boxes: directory, engram, sidecar, write/read paths, third-model strip | arXiv print, reviewers |

Figures 2–3 unchanged (benchmark bars).

## Visual language (top-tier arXiv)

- **Background:** white (print-safe)
- **Typography:** sans-serif labels (DejaVu Sans); monospace only inside directory tree
- **Palette:** muted Tableau-style — blue storage `#4C78A8`, teal seeds `#72B7B2`, amber write `#F28E2B`, green read `#59A14F`, purple graph `#B07AA1`, neutral grey edges `#64748B`
- **Panel labels:** bold `(a)` … `(f)` in upper-left of each axes
- **No decorative title inside figure** — caption lives in LaTeX only
- **Hero:** 8–10 nodes, directed edges, slight edge curvature; faint elliptical “brain casing” at z=0
- **Technical:** each panel in its own `matplotlib` axes via `GridSpec`; no shared absolute coordinates

## Layout grid (12 × 7.5 in)

```
Row 0 (58% height):  [ (a) hero graph 60% ] [ (b) directory + (c) engram + (d) sidecar stacked ]
Row 1 (42% height):  [ (e) experience() ] [ (f) activate() ] [ (g) third data model — 3 columns ]
```

## Content fidelity

Must match `main.tex` caption and engine semantics:

- **Directory:** manifest, hippocampus/, graph/, recall_index.sqlite, WAL checkpoint
- **Engram:** content, provenance, RAG keys, semantic_vector, graph wiring
- **Sidecar:** FTS5 + HNSW seeds
- **experience():** separation → encode → wire graph → upsert sidecar
- **activate():** lexical + semantic seed → graph spread → fuse + provenance boost
- **Third model:** relational vs vector vs FluctlightDB (horizontal comparison, not vertical sidebar)

## Deliverables

| File | Notes |
|------|-------|
| `papers/figures/01-brain-architecture.pdf` | Paper Figure 1 |
| `papers/figures/01-brain-architecture.png` | README / web |
| `papers/figures/01-brain-hero.pdf` | Optional standalone hero (same panel a) |
| `papers/figures/01-brain-hero.png` | GitHub hero asset |
| `papers/figures/generate_all.py` | Regenerator |
| `papers/arxiv-v1/main.tex` | Updated caption if panel letters change |
| `papers/figures/README.md` | Download index |

## Non-goals

- TikZ migration (no local TeX toolchain)
- Animated / interactive figures
- Changing benchmark data or paper claims

## Success criteria

1. Zero overlapping text or boxes at 100% zoom on PDF
2. Readable at `\includegraphics[width=0.98\textwidth]` in two-column figure*
3. Hero visually distinct from “PowerPoint boxes” — graph-first
4. `python3 papers/figures/generate_all.py` reproduces all assets in &lt;30s

## References

- NeurIPS / ICLR system figures: dual-panel (concept graph + schematic)
- Connected Papers: force-directed citation graph, soft node glow
- Current caption: `papers/arxiv-v1/main.tex` Figure 1
