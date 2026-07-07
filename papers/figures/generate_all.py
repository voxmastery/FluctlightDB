#!/usr/bin/env python3
"""Generate all paper figures into papers/figures/ (downloadable from GitHub)."""

from __future__ import annotations

import json
from pathlib import Path

import matplotlib.pyplot as plt
import matplotlib.patches as mpatches
import networkx as nx
import numpy as np
from matplotlib.gridspec import GridSpec
from matplotlib.patches import Ellipse, FancyArrowPatch, FancyBboxPatch

ROOT = Path(__file__).resolve().parent
METRICS = ROOT.parents[1] / "benchmarks" / "results" / "paper-2026-07-07.json"

# Print-safe academic palette
C_STORAGE = "#4C78A8"
C_SEED = "#72B7B2"
C_WRITE = "#F28E2B"
C_READ = "#59A14F"
C_GRAPH = "#B07AA1"
C_ENGRAM = "#9C755F"
C_EDGE = "#64748B"
C_BG_PANEL = "#F8FAFC"
C_BORDER = "#334155"


def save(fig, stem: str) -> None:
    fig.savefig(ROOT / f"{stem}.pdf", bbox_inches="tight", facecolor="white")
    fig.savefig(ROOT / f"{stem}.png", dpi=200, bbox_inches="tight", facecolor="white")
    plt.close(fig)
    print(f"wrote {stem}.pdf / .png")


def _panel_label(ax, letter: str, title: str) -> None:
    ax.text(
        0.02, 0.98, f"({letter}) {title}",
        transform=ax.transAxes, fontsize=10, fontweight="bold",
        va="top", ha="left", color=C_BORDER,
    )


def _rounded_box(ax, xy, w, h, fc, ec=C_BORDER, lw=1.0, alpha=1.0, zorder=1):
    x, y = xy
    patch = FancyBboxPatch(
        (x, y), w, h,
        boxstyle="round,pad=0.012,rounding_size=0.02",
        linewidth=lw, edgecolor=ec, facecolor=fc, alpha=alpha, zorder=zorder,
        transform=ax.transAxes,
    )
    ax.add_patch(patch)
    return patch


def _arrow_axes(ax, p1, p2, color=C_EDGE, style="-|>", lw=1.1):
    arr = FancyArrowPatch(
        p1, p2, arrowstyle=style, mutation_scale=11,
        linewidth=lw, color=color, transform=ax.transAxes, zorder=3,
        connectionstyle="arc3,rad=0.08",
    )
    ax.add_patch(arr)


def fig_hero_graph(ax) -> None:
    """Panel (a): cue-driven activation graph with subtle brain contour."""
    ax.set_xlim(-1.35, 1.35)
    ax.set_ylim(-1.05, 1.15)
    ax.axis("off")
    _panel_label(ax, "a", "Cue-driven memory activation")

    # Brain casing (faint ellipse behind graph)
    brain = Ellipse(
        (0, 0.05), 2.35, 1.85, angle=0,
        facecolor="#EEF2FF", edgecolor="#CBD5E1", linewidth=1.8, alpha=0.55, zorder=0,
    )
    ax.add_patch(brain)
    ax.text(0, 1.02, "agent memory field", ha="center", va="bottom", fontsize=7.5, color="#94A3B8")

    G = nx.DiGraph()
    G.add_edges_from([
        ("Cue", "FTS5"), ("Cue", "HNSW"),
        ("FTS5", "E1"), ("FTS5", "E2"),
        ("HNSW", "E2"), ("HNSW", "E3"),
        ("E1", "G1"), ("E2", "G1"), ("E2", "G2"), ("E3", "G2"),
        ("G1", "Rank"), ("G2", "Rank"),
    ])
    pos = {
        "Cue": (0, 0.82),
        "FTS5": (-0.62, 0.28),
        "HNSW": (0.62, 0.28),
        "E1": (-0.78, -0.22),
        "E2": (0.0, -0.18),
        "E3": (0.78, -0.22),
        "G1": (-0.35, -0.62),
        "G2": (0.35, -0.62),
        "Rank": (0, -0.92),
    }
    node_style = {
        "Cue": (C_STORAGE, 520),
        "FTS5": (C_SEED, 380), "HNSW": (C_SEED, 380),
        "E1": (C_ENGRAM, 340), "E2": (C_ENGRAM, 400), "E3": (C_ENGRAM, 340),
        "G1": (C_GRAPH, 320), "G2": (C_GRAPH, 320),
        "Rank": (C_READ, 460),
    }
    labels_display = {
        "Cue": "cue", "FTS5": "FTS5\nseed", "HNSW": "HNSW\nseed",
        "E1": "engram", "E2": "engram", "E3": "engram",
        "G1": "graph\nspread", "G2": "graph\nspread", "Rank": "fused\nrank",
    }

    for u, v in G.edges():
        x1, y1 = pos[u]
        x2, y2 = pos[v]
        rad = 0.12 if u.startswith("E") else 0.06
        ax.annotate(
            "", xy=(x2, y2), xytext=(x1, y1),
            arrowprops=dict(
                arrowstyle="-|>", color=C_EDGE, lw=1.0,
                connectionstyle=f"arc3,rad={rad}",
                shrinkA=14, shrinkB=14,
            ),
            zorder=1,
        )

    for n, (x, y) in pos.items():
        color, size = node_style[n]
        ax.scatter(x, y, s=size, c=color, edgecolors="white", linewidths=1.5, zorder=2, alpha=0.95)
        ax.text(x, y, labels_display[n], ha="center", va="center", fontsize=7, color="white", fontweight="bold", zorder=3)

    ax.text(-1.2, -1.0, "activate(cue)", fontsize=8, color=C_READ, fontstyle="italic")
    ax.text(0.55, 0.55, "write path\nexperience()", fontsize=7, color=C_WRITE, ha="center",
            bbox=dict(boxstyle="round,pad=0.3", fc="#FFF7ED", ec=C_WRITE, lw=0.8, alpha=0.9))


def fig_technical_stack(ax) -> None:
    """Panels (b)(c)(d): directory, engram, sidecar — stacked, no overlap."""
    ax.set_xlim(0, 1)
    ax.set_ylim(0, 1)
    ax.axis("off")

    _rounded_box(ax, (0.04, 0.66), 0.92, 0.30, C_BG_PANEL)
    ax.text(0.06, 0.93, "(b) Agent brain directory", fontsize=9, fontweight="bold", va="top", color=C_BORDER)
    ax.text(
        0.06, 0.86,
        "manifest.json\n"
        "hippocampus/   engrams\n"
        "graph/         synapses\n"
        "recall_index.sqlite\n"
        "  FTS5 + vector blobs\n"
        "*.wal  →  checkpoint()",
        fontsize=7.5, va="top", family="monospace", color="#1E293B",
    )

    _rounded_box(ax, (0.04, 0.35), 0.92, 0.27, "#F5F3FF")
    ax.text(0.06, 0.59, "(c) Engram (logical unit)", fontsize=9, fontweight="bold", va="top", color=C_BORDER)
    ax.text(
        0.06, 0.53,
        "content · context · outcome\n"
        "provenance: kind, verified\n"
        "rag: doc_id, chunk_id\n"
        "semantic_vector (opt.) · graph wiring",
        fontsize=7.5, va="top", color="#1E293B",
    )

    _rounded_box(ax, (0.04, 0.04), 0.92, 0.26, "#ECFDF5")
    ax.text(0.06, 0.27, "(d) Recall sidecar", fontsize=9, fontweight="bold", va="top", color=C_BORDER)
    ax.text(0.06, 0.20, "FTS5 lexical seeds  +  HNSW semantic seeds", fontsize=7.5, va="top", color="#1E293B")

    _arrow_axes(ax, (0.5, 0.35), (0.5, 0.30), color=C_EDGE)
    ax.text(0.52, 0.325, "index", fontsize=6.5, color="#64748B")


def fig_write_path(ax) -> None:
    ax.set_xlim(0, 1)
    ax.set_ylim(0, 1)
    ax.axis("off")
    _rounded_box(ax, (0.05, 0.08), 0.90, 0.84, "#FFF7ED", ec=C_WRITE)
    _panel_label(ax, "e", "Write: experience()")
    steps = [
        "1. separation gate",
        "2. encode episode",
        "3. wire graph synapses",
        "4. upsert recall sidecar",
    ]
    ax.text(0.10, 0.72, "\n".join(steps), fontsize=8.5, va="top", color="#1E293B", linespacing=1.45)


def fig_read_path(ax) -> None:
    ax.set_xlim(0, 1)
    ax.set_ylim(0, 1)
    ax.axis("off")
    _rounded_box(ax, (0.05, 0.08), 0.90, 0.84, "#ECFDF5", ec=C_READ)
    _panel_label(ax, "f", "Read: activate(cue)")
    steps = [
        "1. lexical + semantic seed",
        "2. graph spread (0–4 hops)",
        "3. fuse FTS5 + vector scores",
        "4. provenance boost → rank",
    ]
    ax.text(0.10, 0.72, "\n".join(steps), fontsize=8.5, va="top", color="#1E293B", linespacing=1.45)


def fig_third_model(ax) -> None:
    ax.set_xlim(0, 1)
    ax.set_ylim(0, 1)
    ax.axis("off")
    _panel_label(ax, "g", "Third data model")

    cols = [
        ("Relational", ["Typed rows", "Predicate match", "No episode graph"], "#F1F5F9"),
        ("Vector DB", ["ANN top-k", "Embedding index", "No provenance recall"], "#F1F5F9"),
        ("FluctlightDB", ["Engram + graph", "Hybrid FTS5+HNSW", "Agent-native contract"], "#EEF2FF"),
    ]
    w = 0.30
    for i, (title, lines, fc) in enumerate(cols):
        x = 0.02 + i * (w + 0.02)
        ec = C_STORAGE if i == 2 else C_BORDER
        lw = 1.4 if i == 2 else 1.0
        _rounded_box(ax, (x, 0.12), w, 0.72, fc, ec=ec, lw=lw)
        ax.text(x + 0.03, 0.78, title, fontsize=8.5, fontweight="bold", va="top", color=C_BORDER)
        ax.text(x + 0.03, 0.68, "\n".join(lines), fontsize=7.2, va="top", color="#1E293B")


def fig_architecture() -> None:
    """Combined Figure 1 via Playwright HTML/SVG (pixel-perfect layout)."""
    from render_fig1_playwright import render

    render("01-brain-architecture")


def fig_hero_standalone() -> None:
    """Standalone hero for README / GitHub (panel a only)."""
    from render_fig1_playwright import render

    render("01-brain-hero", hero_only=True)


def _load_metrics() -> dict:
    return json.loads(METRICS.read_text())


def fig_benchmark_summary() -> None:
    m = _load_metrics()
    lme = m["longmemeval_s"]
    e2e_pct = lme["e2e"]["overall_accuracy"] * 100
    retr_pct = lme["session_recall_at_8"] * 100
    locomo_pct = m["locomo"]["mean_evidence_recall"] * 100
    beir_ndcg = m["beir_scifact"]["systems"]["fluctlightdb_index"]["ndcg_at_10"]
    famb_pct = m["famb"]["index_macro"] * 100

    labels = [
        "LoCoMo\nevidence recall",
        "LongMemEval-S\nsession@8 (retrieval)",
        "LongMemEval-S\nE2E QA (OpenAI)",
        "BEIR SciFact\nnDCG@10",
        "FAMB\nmacro (index)",
    ]
    values = [locomo_pct, retr_pct, e2e_pct, beir_ndcg * 100, famb_pct]
    colors = ["#4C78A8", "#72B7B2", "#59A14F", "#E15759", "#B07AA1"]
    display = [
        f"{locomo_pct:.1f}%",
        f"{retr_pct:.1f}%",
        f"{e2e_pct:.1f}%",
        f"{beir_ndcg:.3f}",
        f"{famb_pct:.0f}%",
    ]

    fig, ax = plt.subplots(figsize=(10, 4.5), facecolor="white")
    bars = ax.bar(labels, values, color=colors, edgecolor=C_BORDER, linewidth=0.8)
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
    lme = _load_metrics()["longmemeval_s"]
    data = lme["by_type"]
    overall_pct = lme["session_recall_at_8"] * 100
    hits = lme["hits"]
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

    fig, ax = plt.subplots(figsize=(9, 4.5), facecolor="white")
    bars = ax.bar(labels, values, color=colors, edgecolor=C_BORDER, linewidth=0.8)
    ax.set_ylim(0, 105)
    ax.set_ylabel("session_recall@8 (%)")
    ax.set_title(
        f"Figure 3 — LongMemEval-S retrieval session@8 by type ({hits} overall = {overall_pct:.1f}%)"
    )
    ax.axhline(90, color="#888", linestyle="--", linewidth=0.8)
    for bar, v in zip(bars, values):
        ax.text(bar.get_x() + bar.get_width() / 2, bar.get_height() + 1,
                f"{v:.1f}%", ha="center", va="bottom", fontsize=10, fontweight="bold")
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)
    fig.tight_layout()
    save(fig, "03-longmemeval-by-type")


def fig_longmemeval_e2e_by_type() -> None:
    e2e = _load_metrics()["longmemeval_s"]["e2e"]
    data = e2e["by_type_accuracy"]
    overall_pct = e2e["overall_accuracy"] * 100
    n = e2e["questions"]
    correct = round(overall_pct / 100 * n)

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

    fig, ax = plt.subplots(figsize=(9, 4.5), facecolor="white")
    bars = ax.bar(labels, values, color=colors, edgecolor=C_BORDER, linewidth=0.8)
    ax.set_ylim(0, 105)
    ax.set_ylabel("E2E QA accuracy (%)")
    ax.set_title(
        f"Figure 4 — LongMemEval-S E2E QA by type (paper profile; {correct}/{n} = {overall_pct:.1f}%)"
    )
    ax.axhline(90, color="#888", linestyle="--", linewidth=0.8)
    for bar, v in zip(bars, values):
        ax.text(
            bar.get_x() + bar.get_width() / 2,
            bar.get_height() + 1,
            f"{v:.1f}%",
            ha="center",
            va="bottom",
            fontsize=10,
            fontweight="bold",
        )
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)
    fig.tight_layout()
    save(fig, "04-longmemeval-e2e-by-type")


def main() -> None:
    ROOT.mkdir(parents=True, exist_ok=True)
    fig_architecture()
    fig_hero_standalone()
    fig_benchmark_summary()
    fig_longmemeval_by_type()
    fig_longmemeval_e2e_by_type()
    print(f"All figures in {ROOT}")


if __name__ == "__main__":
    main()
