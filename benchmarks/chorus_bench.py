#!/usr/bin/env python3
"""CHORUS Lane benchmarks — imprint speed, recall quality, sleep collapse.

Run:
  PYTHONPATH=sdks/python python benchmarks/chorus_bench.py
  PYTHONPATH=sdks/python python benchmarks/chorus_bench.py --beir
"""

from __future__ import annotations

import argparse
import json
import os
import pickle
import statistics
import sys
import time
from pathlib import Path
from typing import Any

REPO = Path(__file__).resolve().parents[1]
SDK = REPO / "sdks" / "python"
if str(SDK) not in sys.path:
    sys.path.insert(0, str(SDK))

TOPK = int(os.environ.get("BEIR_TOPK", "100"))


def micro_bench() -> dict[str, Any]:
    from fluctlightdb import connect_chorus

    brain = connect_chorus()
    n = 2000
    batch = [
        {
            "memory_id": f"m{i}",
            "content": f"topic {i} alpha beta gamma document text",
            "context": f"ctx{i}",
            "semantic_vector": [((i * 17 + j) % 100) / 100.0 - 0.5 for j in range(32)],
            "salience": 0.6,
        }
        for i in range(n)
    ]
    t0 = time.perf_counter()
    imprinted = brain.chorus_imprint_batch(batch)
    ingest_s = time.perf_counter() - t0
    lats = []
    for i in range(50):
        t1 = time.perf_counter()
        brain.chorus_recall(f"topic {i}", limit=10, semantic_vector=batch[i]["semantic_vector"])
        lats.append((time.perf_counter() - t1) * 1000.0)
    sleep_report = brain.chorus_sleep()
    return {
        "imprinted": imprinted,
        "ingest_ms_per_item": round(ingest_s / n * 1000.0, 4),
        "ingest_wall_s": round(ingest_s, 3),
        "recall_ms_p50": round(statistics.median(lats), 3),
        "recall_ms_p95": round(sorted(lats)[int(len(lats) * 0.95) - 1], 3),
        "chorus_len": brain.chorus_len(),
        "sleep": sleep_report,
        "hippocampus_engrams": brain.status().get("engrams", 0)
        if hasattr(brain, "status")
        else None,
    }


def beir_bench() -> dict[str, Any]:
    from beir_bench import (
        BEIR_ROOT,
        BEIR_DS,
        ensure_beir_dataset,
        load_jsonl,
        load_qrels,
        run_chroma,
        score_run,
    )
    from fluctlightdb import connect_chorus

    ds_dir = ensure_beir_dataset(BEIR_ROOT, BEIR_DS)
    corpus = load_jsonl(ds_dir / "corpus.jsonl")
    queries = load_jsonl(ds_dir / "queries.jsonl")
    qrels = load_qrels(ds_dir / "qrels" / "test.tsv")
    doc_ids = list(corpus.keys())
    doc_texts = [
        f"{corpus[d].get('title', '')} {corpus[d].get('text', '')}".strip() for d in doc_ids
    ]
    test_qids = sorted(qrels.keys())
    q_texts = [queries[q].get("text", "") for q in test_qids]
    cache = BEIR_ROOT / f"{BEIR_DS}_vecs.pkl"
    cached = pickle.loads(cache.read_bytes())
    doc_vecs = cached["doc_vecs"]
    q_vecs = cached["q_vecs"]

    brain = connect_chorus()
    batch = [
        {
            "memory_id": did,
            "content": txt,
            "context": did,
            "semantic_vector": vec,
            "salience": 0.6,
        }
        for did, txt, vec in zip(doc_ids, doc_texts, doc_vecs)
    ]
    t0 = time.perf_counter()
    brain.chorus_imprint_batch(batch)
    imprint_s = time.perf_counter() - t0

    run: dict[str, dict[str, float]] = {}
    dim = len(q_vecs[0]) if q_vecs else 0
    t1 = time.perf_counter()
    batch_hits = brain.chorus_recall_batch(q_texts, q_vecs, limit=TOPK)
    batch_s = time.perf_counter() - t1
    per_query_ms = batch_s / max(len(test_qids), 1) * 1000.0
    # single-query p50 (first 100) for latency SLO
    single_lats: list[float] = []
    for qtxt, qv in zip(q_texts[:100], q_vecs[:100]):
        tq = time.perf_counter()
        brain.chorus_recall(qtxt, limit=TOPK, semantic_vector=qv)
        single_lats.append((time.perf_counter() - tq) * 1000.0)
    single_p50 = statistics.median(single_lats) if single_lats else per_query_ms
    for qid, hits in zip(test_qids, batch_hits):
        scores: dict[str, float] = {}
        for rank, h in enumerate(hits):
            if isinstance(h, (list, tuple)) and len(h) >= 2:
                mid, score = str(h[0]), float(h[1])
            elif isinstance(h, dict):
                mid = str(h.get("memory_id", ""))
                score = float(h.get("score") or (TOPK - rank))
            else:
                continue
            if mid:
                scores[mid] = score
        run[qid] = scores

    sleep_report = brain.chorus_sleep()

    chroma_run, chroma_q_ms, chroma_w_ms = run_chroma(
        doc_ids, doc_texts, doc_vecs, test_qids, q_texts, q_vecs
    )
    lat_sorted = sorted([per_query_ms] * len(test_qids))
    return {
        "benchmark": "chorus_beir",
        "dataset": BEIR_DS,
        "docs": len(doc_ids),
        "queries": len(test_qids),
        "chorus": {
            **score_run(run, qrels),
            "imprint_ms_per_doc": round(imprint_s / len(doc_ids) * 1000.0, 3),
            "query_ms_mean": round(per_query_ms, 3),
            "query_ms_p50": round(single_p50, 3),
            "query_ms_p95": round(sorted(single_lats)[max(0, int(len(single_lats) * 0.95) - 1)], 3)
            if single_lats
            else round(per_query_ms, 3),
            "query_batch_wall_s": round(batch_s, 3),
            "imprint_wall_s": round(imprint_s, 2),
            "sleep": sleep_report,
            "lane": "grg",
        },
        "chroma": {
            **score_run(chroma_run, qrels),
            "write_ms_per_doc": round(chroma_w_ms, 2),
            "query_ms": round(chroma_q_ms, 2),
        },
    }


def main() -> int:
    ap = argparse.ArgumentParser(description="CHORUS Lane benchmarks")
    ap.add_argument("--beir", action="store_true", help="Run BEIR SciFact comparison")
    ap.add_argument("--json-out", type=Path, default=None)
    args = ap.parse_args()

    if args.beir:
        sys.path.insert(0, str(REPO / "benchmarks"))
        out = beir_bench()
    else:
        out = micro_bench()

    print(json.dumps(out, indent=2))
    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(json.dumps(out, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
