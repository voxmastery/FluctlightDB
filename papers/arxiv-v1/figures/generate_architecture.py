#!/usr/bin/env python3
"""Generate FluctlightDB architecture figure for the arXiv paper."""

from __future__ import annotations

from pathlib import Path

import matplotlib.pyplot as plt
from matplotlib.patches import FancyArrowPatch, FancyBboxPatch

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "figures" / "brain-architecture.pdf"


def box(ax, xy, w, h, title, lines, fc, ec="#333333"):
    x, y = xy
    patch = FancyBboxPatch(
        (x, y),
        w,
        h,
        boxstyle="round,pad=0.02,rounding_size=0.02",
        linewidth=1.2,
        edgecolor=ec,
        facecolor=fc,
    )
    ax.add_patch(patch)
    ax.text(x + 0.02, y + h - 0.05, title, fontsize=11, fontweight="bold", va="top")
    body = "\n".join(lines)
    ax.text(x + 0.02, y + h - 0.11, body, fontsize=8.5, va="top", family="monospace")


def arrow(ax, p1, p2, text=None):
    arr = FancyArrowPatch(
        p1,
        p2,
        arrowstyle="-|>",
        mutation_scale=12,
        linewidth=1.2,
        color="#444444",
        connectionstyle="arc3,rad=0.0",
    )
    ax.add_patch(arr)
    if text:
        mx, my = (p1[0] + p2[0]) / 2, (p1[1] + p2[1]) / 2
        ax.text(mx, my + 0.02, text, fontsize=7.5, ha="center", color="#555555")


def main() -> None:
    fig, ax = plt.subplots(figsize=(11, 6.2))
    ax.set_xlim(0, 1)
    ax.set_ylim(0, 1)
    ax.axis("off")

    # (a) persistence
    box(
        ax,
        (0.04, 0.52),
        0.34,
        0.38,
        "(a) Agent brain directory",
        [
            "manifest.json",
            "hippocampus/  engrams",
            "graph/        synapses",
            "recall_index.sqlite",
            "  FTS5 + vector blobs",
            "*.wal  -> checkpoint()",
        ],
        fc="#e8f0ff",
    )

    # (b) engram
    box(
        ax,
        (0.44, 0.62),
        0.28,
        0.28,
        "(b) Engram (logical unit)",
        [
            "content, context, outcome",
            "provenance: kind, verified",
            "rag: doc_id, chunk_id",
            "semantic_vector (opt.)",
            "graph neuron ensemble",
        ],
        fc="#f3e8ff",
    )

    # sidecar
    box(
        ax,
        (0.44, 0.38),
        0.28,
        0.18,
        "Recall sidecar",
        ["FTS5 lexical seeds", "HNSW semantic seeds"],
        fc="#e8f8ec",
    )

    arrow(ax, (0.58, 0.62), (0.58, 0.56), "index")

    # (c) write
    box(
        ax,
        (0.08, 0.08),
        0.22,
        0.22,
        "(c) Write: experience()",
        ["separation gate", "encode episode", "wire graph", "upsert sidecar"],
        fc="#fff3e6",
    )

    # (d) read
    box(
        ax,
        (0.36, 0.08),
        0.22,
        0.22,
        "(d) Read: activate(cue)",
        ["lexical + semantic seed", "graph spread (0-4 hops)", "fuse scores", "provenance boost"],
        fc="#fff3e6",
    )

    arrow(ax, (0.20, 0.30), (0.20, 0.52))
    arrow(ax, (0.47, 0.30), (0.50, 0.38))
    arrow(ax, (0.30, 0.19), (0.36, 0.19), "same store")

    # contrast bar
    box(
        ax,
        (0.64, 0.10),
        0.32,
        0.80,
        "Third data model",
        [
            "Relational: typed rows,",
            "  no provenance recall",
            "",
            "Vector DB: ANN top-k,",
            "  no episode graph",
            "",
            "FluctlightDB: engram +",
            "  graph + hybrid index",
            "  in one contract",
        ],
        fc="#f5f5f5",
    )

    ax.text(
        0.5,
        0.98,
        "FluctlightDB persistence and recall layout",
        ha="center",
        va="top",
        fontsize=13,
        fontweight="bold",
    )
    ax.text(
        0.5,
        0.94,
        "One embedded directory per agent — experience() writes; activate() recalls",
        ha="center",
        va="top",
        fontsize=9,
        color="#555555",
    )

    OUT.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(OUT, bbox_inches="tight")
    fig.savefig(OUT.with_suffix(".png"), dpi=200, bbox_inches="tight")
    print(f"wrote {OUT}")
    print(f"wrote {OUT.with_suffix('.png')}")


if __name__ == "__main__":
    main()
