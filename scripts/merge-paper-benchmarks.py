#!/usr/bin/env python3
"""Merge fresh LoCoMo / BEIR / FAMB JSON into paper freeze file."""

from __future__ import annotations

import argparse
import json
from datetime import date
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]


def load(path: Path) -> dict:
    return json.loads(path.read_text())


def round3(x: float) -> float:
    return round(float(x), 3)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--paper", type=Path, default=REPO / "benchmarks/results/paper-2026-07-09.json")
    ap.add_argument("--locomo", type=Path, required=True)
    ap.add_argument("--beir", type=Path, required=True)
    ap.add_argument("--famb-agent", type=Path, required=True)
    ap.add_argument("--famb-chorus", type=Path, required=True)
    ap.add_argument("--out", type=Path, default=None)
    args = ap.parse_args()

    paper = load(args.paper)
    locomo = load(args.locomo)
    beir = load(args.beir)
    famb_a = load(args.famb_agent)
    famb_c = load(args.famb_chorus)

    chroma = beir["chroma"]
    fl_ch = beir["fluctlightdb_chorus"]
    chorus = {
        "ndcg_at_10": round3(fl_ch["ndcg_at_10"]),
        "recall_at_10": round3(fl_ch["recall_at_10"]),
        "recall_at_100": round3(fl_ch["recall_at_100"]),
        "query_ms": str(fl_ch.get("query_ms", fl_ch.get("query_ms_mean", ""))),
        "imprint_ms_per_doc": fl_ch.get("imprint_ms_per_doc"),
        "lane": fl_ch.get("lane", "chorus_grg_ir"),
        "frozen_source": str(args.beir.name),
    }

    paper["date"] = date.today().isoformat()
    paper["locomo"] = {
        "conversations": locomo.get("conversations", 10),
        "gold_evidence_spans": locomo.get("questions", 1982),
        "top_k": locomo.get("top_k", 150),
        "mode": locomo.get("mode", "chorus"),
        "lane": locomo.get("lane", "chorus_grg"),
        "rag_mode": locomo.get("rag_mode", "all"),
        "embedder": locomo.get("embedder", "all-MiniLM-L6-v2 ONNX CPU"),
        "mean_evidence_recall": round3(locomo["mean_evidence_recall"]),
        "evidence_all_in_context": round3(locomo["evidence_all_in_context"]),
        "evidence_hits": locomo["evidence_hits"],
        "wall_s_warm": round(locomo.get("wall_s", 0), 1),
        "wall_s_cold": round(locomo.get("wall_s", 0), 1),
        "memories_ingested": locomo.get("memories_ingested"),
        "frozen_source": str(args.locomo.name),
    }

    paper["beir_scifact"] = {
        "embedder": "all-MiniLM-L6-v2 ONNX CPU",
        "systems": {
            "chroma": {
                "ndcg_at_10": round3(chroma["ndcg_at_10"]),
                "recall_at_10": round3(chroma["recall_at_10"]),
                "recall_at_100": round3(chroma["recall_at_100"]),
                "query_ms": f"{round(chroma['query_ms']):.0f}",
            },
            "fluctlightdb_chorus": chorus,
        },
        "frozen_source": str(args.beir.name),
        "chorus_frozen_source": str(args.beir.name),
    }

    paper["famb"] = {
        "agent_macro": round3(famb_a["macro"]),
        "chorus_macro": round3(famb_c["macro"]),
        "agent_lane": famb_a.get("lane", "agent_unified"),
        "chorus_lane": famb_c.get("lane", "chorus_grg"),
        "agent_scores": famb_a.get("scores"),
        "chorus_scores": famb_c.get("scores"),
        "frozen_source_agent": str(args.famb_agent.name),
        "frozen_source_chorus": str(args.famb_chorus.name),
    }

    out = args.out or args.paper
    out.write_text(json.dumps(paper, indent=2) + "\n")
    print(f"Wrote {out}")
    print(
        f"LoCoMo {paper['locomo']['mean_evidence_recall']:.1%} ({paper['locomo']['lane']}) | "
        f"BEIR chorus {chorus['ndcg_at_10']:.3f} vs chroma {chroma['ndcg_at_10']:.3f} | "
        f"FAMB agent {paper['famb']['agent_macro']:.0%} chorus {paper['famb']['chorus_macro']:.0%}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
