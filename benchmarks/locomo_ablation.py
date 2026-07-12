#!/usr/bin/env python3
"""LoCoMo ablations: recall@k sensitivity and hybrid vs vector-only (index lane).

Usage:
  PYTHONPATH=sdks/python python benchmarks/locomo_ablation.py --sweep-k
  PYTHONPATH=sdks/python python benchmarks/locomo_ablation.py --hybrid-vs-vector
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from pathlib import Path
from typing import Any

REPO = Path(__file__).resolve().parents[1]
SDK = REPO / "sdks/python"
if str(SDK) not in sys.path:
    sys.path.insert(0, str(SDK))

from bench_lanes import configure_ir_env, open_lane  # noqa: E402
from locomo_eval import (  # noqa: E402
    DEFAULT_DATA,
    EmbedCache,
    CACHE_DIR,
    collect_turns,
    eval_conversation_chorus,
    load_locomo,
    normalize_evidence,
    recalled_dia_ids_from_chorus,
    score_evidence,
)


from locomo_metrics import summarize_hits  # noqa: E402


def eval_chorus_at_k(
    brain: Any,
    item: dict,
    embedder: EmbedCache,
    *,
    max_k: int,
    target_k: int,
    neighbor_window: int = 3,
) -> list[dict]:
    """Reuse a single chorus_recall_batch at max_k; derive metrics for target_k."""
    qas = [
        qa
        for qa in (item.get("qa") or [])
        if isinstance(qa, dict) and (qa.get("question") or "").strip() and qa.get("evidence")
    ]
    if not qas:
        return []
    questions = [(qa.get("question") or "").strip()[:400] for qa in qas]
    q_vecs = embedder.get_many(questions)
    batch = brain.chorus_recall_batch(questions, q_vecs, limit=max_k)
    rows: list[dict] = []
    for qa, hits in zip(qas, batch):
        truncated = (hits or [])[:target_k]
        found = recalled_dia_ids_from_chorus(truncated, target_k)
        evidence = normalize_evidence(qa.get("evidence") or [])
        scored = score_evidence(
            found, evidence, item, neighbor_window=neighbor_window
        )
        rows.append(
            {
                "question": (qa.get("question") or "").strip(),
                "recall_n": len(truncated),
                **scored,
            }
        )
    return rows


def run_k_sweep(
    items: list[dict],
    embedder: EmbedCache,
    ks: list[int],
    *,
    fabric: bool,
) -> dict[str, Any]:
    if fabric:
        configure_ir_env()
    else:
        os.environ.pop("FLUCTLIGHT_FABRIC", None)
        os.environ["FLUCTLIGHT_FABRIC"] = "0"
        configure_ir_env()
    max_k = max(ks)
    per_k_rows: dict[int, list[dict]] = {k: [] for k in ks}
    from locomo_eval import ingest_chorus

    t0 = time.perf_counter()
    from locomo_eval import ingest_chorus

    for item in items:
        brain = open_lane("chorus")
        ingest_chorus(brain, item, embedder, rag_mode="all")
        qas = [
            qa
            for qa in (item.get("qa") or [])
            if isinstance(qa, dict) and (qa.get("question") or "").strip() and qa.get("evidence")
        ]
        if not qas:
            continue
        questions = [(qa.get("question") or "").strip()[:400] for qa in qas]
        q_vecs = embedder.get_many(questions)
        batch = brain.chorus_recall_batch(questions, q_vecs, limit=max_k)
        for k in ks:
            rows: list[dict] = []
            for qa, hits in zip(qas, batch):
                truncated = (hits or [])[:k]
                found = recalled_dia_ids_from_chorus(truncated, k)
                evidence = normalize_evidence(qa.get("evidence") or [])
                rows.append(score_evidence(found, evidence, item, neighbor_window=3))
            per_k_rows[k].extend(rows)

    out: dict[str, Any] = {
        "benchmark": "locomo",
        "ablation": "recall_at_k",
        "fabric_on": fabric,
        "ks": ks,
        "neighbor_window": 3,
        "by_k": {},
        "wall_s": round(time.perf_counter() - t0, 1),
    }
    for k in ks:
        summary = summarize_hits(per_k_rows[k])
        out["by_k"][str(k)] = {
            "mean_evidence_recall": summary["mean_evidence_recall"],
            "mean_evidence_recall_raw": summary["mean_evidence_recall_raw"],
            "mean_evidence_recall_expanded": summary["mean_evidence_recall_expanded"],
            "evidence_all_in_context": summary["evidence_all_in_context"],
            "evidence_hits": summary.get("evidence_hits"),
            "questions": summary["questions"],
        }
    return out


def ingest_index_turns(brain: Any, item: dict, embedder: EmbedCache, *, rag_mode: str) -> int:
    rows = collect_turns(item, rag_mode)
    n = 0
    for row in rows:
        vec = embedder.get_many([row["body"][:800]])[0]
        brain.experience(
            row["body"],
            context=row.get("chunk_id") or row["dia"],
            salience=0.72,
            semantic_vector=vec,
            rag_doc_id=row["dia"],
            rag_chunk_id=row.get("chunk_id") or row["dia"],
        )
        n += 1
    return n


def eval_index_hybrid(
    brain: Any,
    item: dict,
    embedder: EmbedCache,
    *,
    top_k: int,
    vector_only: bool,
) -> list[dict]:
    if vector_only:
        os.environ["FLUCTLIGHT_VECTOR_FAST"] = "1"
        os.environ["FLUCTLIGHT_AGENT_FAST"] = "1"
    else:
        os.environ.pop("FLUCTLIGHT_VECTOR_FAST", None)
        os.environ.pop("FLUCTLIGHT_AGENT_FAST", None)

    qas = [
        qa
        for qa in (item.get("qa") or [])
        if isinstance(qa, dict) and (qa.get("question") or "").strip() and qa.get("evidence")
    ]
    rows: list[dict] = []
    for qa in qas:
        question = (qa.get("question") or "").strip()[:400]
        qvec = embedder.get_many([question])[0]
        raw = brain.activate(question, semantic_vector=qvec, limit=top_k)
        recalls = raw.get("recalls") if isinstance(raw, dict) else raw
        recalled: set[str] = set()
        for r in (recalls or [])[:top_k]:
            ep = r.get("episode") or {}
            rag = ep.get("rag") or {}
            doc = rag.get("doc_id") or ep.get("doc_id")
            if doc and str(doc).startswith("D"):
                recalled.add(str(doc))
            content = str(ep.get("content") or "")
            import re

            recalled.update(re.findall(r"\bD\d+:\d+\b", content))
        evidence = normalize_evidence(qa.get("evidence") or [])
        rows.append(score_evidence(recalled, evidence, item, neighbor_window=3))
    return rows


def run_hybrid_vs_vector(items: list[dict], embedder: EmbedCache, *, top_k: int) -> dict[str, Any]:
    os.environ.pop("FLUCTLIGHT_FABRIC", None)
    os.environ["FLUCTLIGHT_FABRIC"] = "0"
    profiles = {
        "hybrid_bm25_vector": False,
        "vector_only_fast": True,
    }
    out: dict[str, Any] = {
        "benchmark": "locomo",
        "ablation": "hybrid_vs_vector_index",
        "lane": "connect_index",
        "top_k": top_k,
        "profiles": {},
    }
    t0 = time.perf_counter()
    for name, vector_only in profiles.items():
        all_rows: list[dict] = []
        for item in items:
            brain = open_lane("agent")  # index-capable path via connect in bench?
            # connect_index explicitly
            from fluctlightdb.brain import FluctlightBrain

            brain = FluctlightBrain.connect_index()
            ingest_index_turns(brain, item, embedder, rag_mode="all")
            all_rows.extend(
                eval_index_hybrid(brain, item, embedder, top_k=top_k, vector_only=vector_only)
            )
        summary = summarize_hits(all_rows)
        out["profiles"][name] = {
            "mean_evidence_recall": summary["mean_evidence_recall"],
            "mean_evidence_recall_raw": summary["mean_evidence_recall_raw"],
            "mean_evidence_recall_expanded": summary["mean_evidence_recall_expanded"],
            "evidence_all_in_context": summary["evidence_all_in_context"],
            "evidence_hits": summary.get("evidence_hits"),
        }
    out["wall_s"] = round(time.perf_counter() - t0, 1)
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description="LoCoMo ablation harness")
    ap.add_argument("--data", type=Path, default=DEFAULT_DATA)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--sweep-k", action="store_true")
    ap.add_argument("--hybrid-vs-vector", action="store_true")
    ap.add_argument("--ks", default="5,10,25,50,150")
    ap.add_argument("--top-k", type=int, default=50)
    ap.add_argument("--json-out", type=Path, required=True)
    ap.add_argument("--no-fabric", action="store_true", help="k-sweep with fabric off")
    args = ap.parse_args()

    items = load_locomo(args.data)
    if args.limit > 0:
        items = items[: args.limit]
    embedder = EmbedCache(CACHE_DIR / "minilm_vecs.pkl")

    if args.sweep_k:
        ks = [int(x) for x in args.ks.split(",") if x.strip()]
        out = run_k_sweep(items, embedder, ks, fabric=not args.no_fabric)
    elif args.hybrid_vs_vector:
        out = run_hybrid_vs_vector(items, embedder, top_k=args.top_k)
    else:
        ap.error("pass --sweep-k and/or --hybrid-vs-vector")

    embedder.save()
    args.json_out.parent.mkdir(parents=True, exist_ok=True)
    args.json_out.write_text(json.dumps(out, indent=2) + "\n")
    print(json.dumps(out, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
