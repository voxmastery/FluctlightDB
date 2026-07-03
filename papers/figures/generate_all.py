#!/usr/bin/env python3
"""Generate all paper figures into papers/figures/ (downloadable from GitHub)."""

from __future__ import annotations

import json
from pathlib import Path

import matplotlib.pyplot as plt
import matplotlib.patches as mpatches
from matplotlib.patches import FancyArrowPatch, FancyBboxPatch

ROOT = Path(__file__).resolve().parent
METRICS = ROOT.parents[1] / "benchmarks" / "results" / "paper-2026-07-03.json"


def save(fig, stem: str) -> None:
    fig.savefig(ROOT / f"{stem}.pdf", bbox_inches="tight")
    fig.savefig(ROOT / f"{stem}.png", dpi=200, bbox_inches="tight")
    plt.close(fig)
    print(f"wrote {stem}.pdf / .png")


def fig_architecture() -> None:
    fig, ax = plt.subplots(figsize=(11, 6.2))
    ax.set_xlim(0, 1)
    ax.set_ylim(0, 1)
    ax.axis("off")

    def box(xy, w, h, title, lines, fc):
        x, y = xy
        patch = FancyBboxPatch(
            (x, y), w, h,
            boxstyle="round,pad=0.02,rounding_size=0.02",
            linewidth=1.2, edgecolor="#333", facecolor=fc,
        )
        ax.add_patch(patch)
        ax.text(x + 0.02, y + h - 0.05, title, fontsize=11, fontweight="bold", va="top")
        ax.text(x + 0.02, y + h - 0.11, "\n".join(lines), fontsize=8.5, va="top", family="monospace")

    def arrow(p1, p2, text=None):
        arr = FancyArrowPatch(p1, p2, arrowstyle="-|>", mutation_scale=12, linewidth=1.2, color="#444")
        ax.add_patch(arr)
        if text:
            mx, my = (p1[0] + p2[0]) / 2, (p1[1] + p2[1]) / 2
            ax.text(mx, my + 0.02, text, fontsize=7.5, ha="center", color="#555")

    box((0.04, 0.52), 0.34, 0.38, "(a) Agent brain directory",
        ["manifest.json", "hippocampus/  engrams", "graph/        synapses",
         "recall_index.sqlite", "  FTS5 + vector blobs", "*.wal  -> checkpoint()"], "#e8f0ff")
    box((0.44, 0.62), 0.28, 0.28, "(b) Engram (logical unit)",
        ["content, context, outcome", "provenance: kind, verified", "rag: doc_id, chunk_id",
         "semantic_vector (opt.)", "graph neuron ensemble"], "#f3e8ff")
    box((0.44, 0.38), 0.28, 0.18, "Recall sidecar",
        ["FTS5 lexical seeds", "HNSW semantic seeds"], "#e8f8ec")
    arrow((0.58, 0.62), (0.58, 0.56), "index")
    box((0.08, 0.08), 0.22, 0.22, "(c) Write: experience()",
        ["separation gate", "encode episode", "wire graph", "upsert sidecar"], "#fff3e6")
    box((0.36, 0.08), 0.22, 0.22, "(d) Read: activate(cue)",
        ["lexical + semantic seed", "graph spread (0-4 hops)", "fuse scores", "provenance boost"], "#fff3e6")
    arrow((0.20, 0.30), (0.20, 0.52))
    arrow((0.47, 0.30), (0.50, 0.38))
    arrow((0.30, 0.19), (0.36, 0.19), "same store")
    box((0.64, 0.10), 0.32, 0.80, "Third data model",
        ["Relational: typed rows,", "  no provenance recall", "",
         "Vector DB: ANN top-k,", "  no episode graph", "",
         "FluctlightDB: engram +", "  graph + hybrid index", "  in one contract"], "#f5f5f5")
    ax.text(0.5, 0.98, "Figure 1 — FluctlightDB persistence and recall layout",
            ha="center", va="top", fontsize=13, fontweight="bold")
    ax.text(0.5, 0.94, "One embedded directory per agent — experience() writes; activate() recalls",
            ha="center", va="top", fontsize=9, color="#555")
    save(fig, "01-brain-architecture")


def fig_benchmark_summary() -> None:
    labels = ["LoCoMo\nevidence recall", "LongMemEval-S\nsession@8", "BEIR SciFact\nnDCG@10", "FAMB\nmacro (index)"]
    values = [98.1, 96.8, 64.5, 98.0]  # nDCG scaled to % for visual comparison
    colors = ["#4C78A8", "#59A14F", "#E15759", "#B07AA1"]
    display = ["98.1%", "96.8%", "0.645", "98%"]

    fig, ax = plt.subplots(figsize=(8, 4.5))
    bars = ax.bar(labels, values, color=colors, edgecolor="#333", linewidth=0.8)
    ax.set_ylim(0, 105)
    ax.set_ylabel("Score (% scale; BEIR nDCG×100)")
    ax.set_title("Figure 2 — FluctlightDB headline benchmark results (July 2026)")
    ax.axhline(90, color="#888", linestyle="--", linewidth=0.8, label="90% reference")
    for bar, txt in zip(bars, display):
        ax.text(bar.get_x() + bar.get_width() / 2, bar.get_height() + 1.5, txt,
                ha="center", va="bottom", fontsize=11, fontweight="bold")
    ax.legend(loc="lower right", fontsize=8)
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)
    fig.tight_layout()
    save(fig, "02-benchmark-summary")


def fig_longmemeval_by_type() -> None:
    data = json.loads(METRICS.read_text())["longmemeval_s"]["by_type"]
    order = [
        ("knowledge-update", "knowledge\nupdate"),
        ("multi-session", "multi-\nsession"),
        ("single-session-user", "user"),
        ("single-session-assistant", "assistant"),
        ("temporal-reasoning", "temporal"),
        ("single-session-preference", "preference"),
    ]
    labels = [o[1] for o in order]
    values = [data[o[0]] * 100 for o in order]
    colors = ["#59A14F" if v >= 90 else "#F28E2B" if v >= 80 else "#E15759" for v in values]

    fig, ax = plt.subplots(figsize=(9, 4.5))
    bars = ax.bar(labels, values, color=colors, edgecolor="#333", linewidth=0.8)
    ax.set_ylim(0, 105)
    ax.set_ylabel("session_recall@8 (%)")
    ax.set_title("Figure 3 — LongMemEval-S by question type (484/500 overall = 96.8%)")
    ax.axhline(90, color="#888", linestyle="--", linewidth=0.8)
    for bar, v in zip(bars, values):
        ax.text(bar.get_x() + bar.get_width() / 2, bar.get_height() + 1,
                f"{v:.1f}%", ha="center", va="bottom", fontsize=10, fontweight="bold")
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)
    fig.tight_layout()
    save(fig, "03-longmemeval-by-type")


def main() -> None:
    ROOT.mkdir(parents=True, exist_ok=True)
    fig_architecture()
    fig_benchmark_summary()
    fig_longmemeval_by_type()
    print(f"All figures in {ROOT}")


if __name__ == "__main__":
    main()
