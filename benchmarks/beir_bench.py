#!/usr/bin/env python3
"""Certified IR benchmark: BEIR + pytrec_eval (nDCG@10, Recall@10/100).

Compares Chroma vs FluctlightDB **CHORUS** lane on shared all-MiniLM-L6-v2 embeddings.

Usage:
  BEIR_DATA=/tmp/beir BEIR_DS=scifact \\
  PYTHONPATH=sdks/python python benchmarks/beir_bench.py \\
    --json-out benchmarks/results/beir-scifact-2026-07-09.json
"""

from __future__ import annotations

import argparse
import json
import os
import pickle
import statistics
import sys
import time
import urllib.request
import zipfile
from pathlib import Path
from typing import Any

REPO = Path(__file__).resolve().parents[1]
SDK = REPO / "sdks/python"
if str(SDK) not in sys.path:
    sys.path.insert(0, str(SDK))

from bench_lanes import configure_ir_env, open_lane  # noqa: E402

BEIR_ROOT = Path(os.environ.get("BEIR_DATA", "/tmp/beir"))
BEIR_DS = os.environ.get("BEIR_DS", "scifact")
BEIR_URL = (
    "https://public.ukp.informatik.tu-darmstadt.de/thakur/BEIR/datasets/scifact.zip"
)
TOPK = int(os.environ.get("BEIR_TOPK", "100"))


def load_jsonl(path: Path) -> dict[str, dict]:
    out: dict[str, dict] = {}
    with path.open() as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            obj = json.loads(line)
            out[str(obj["_id"])] = obj
    return out


def load_qrels(path: Path) -> dict[str, dict[str, int]]:
    qrels: dict[str, dict[str, int]] = {}
    with path.open() as f:
        for i, line in enumerate(f):
            if i == 0 and line.lower().startswith("query-id"):
                continue
            parts = line.strip().split()
            if len(parts) < 3:
                continue
            qid, docid, rel = parts[0], parts[1], int(parts[2])
            qrels.setdefault(qid, {})[docid] = rel
    return qrels


def ensure_beir_dataset(root: Path, ds: str) -> Path:
    ds_dir = root / ds
    if (ds_dir / "corpus.jsonl").is_file():
        return ds_dir
    root.mkdir(parents=True, exist_ok=True)
    zip_path = root / f"{ds}.zip"
    if not zip_path.is_file():
        print(f"Downloading BEIR {ds}...", file=sys.stderr)
        urllib.request.urlretrieve(BEIR_URL, zip_path)
    with zipfile.ZipFile(zip_path, "r") as zf:
        zf.extractall(root)
    return ds_dir


def score_run(run: dict[str, dict[str, float]], qrels: dict[str, dict[str, int]]) -> dict[str, float]:
    import pytrec_eval

    evaluator = pytrec_eval.RelevanceEvaluator(
        qrels, {"ndcg_cut_10", "recall_10", "recall_100"}
    )
    results = evaluator.evaluate(run)
    ndcg10 = [m.get("ndcg_cut_10", 0.0) for m in results.values()]
    r10 = [m.get("recall_10", 0.0) for m in results.values()]
    r100 = [m.get("recall_100", 0.0) for m in results.values()]
    return {
        "ndcg_at_10": statistics.mean(ndcg10) if ndcg10 else 0.0,
        "recall_at_10": statistics.mean(r10) if r10 else 0.0,
        "recall_at_100": statistics.mean(r100) if r100 else 0.0,
    }


def run_chroma(
    doc_ids: list[str],
    doc_texts: list[str],
    doc_vecs: list[list[float]],
    test_qids: list[str],
    q_vecs: list[list[float]],
) -> tuple[dict[str, dict[str, float]], float, float]:
    import chromadb

    t0 = time.perf_counter()
    cli = chromadb.EphemeralClient()
    col = cli.create_collection("beircol", metadata={"hnsw:space": "cosine"})
    col.add(ids=doc_ids, documents=doc_texts, embeddings=doc_vecs)
    write_ms = (time.perf_counter() - t0) / len(doc_ids) * 1000.0
    run: dict[str, dict[str, float]] = {}
    lats: list[float] = []
    for qid, qv in zip(test_qids, q_vecs):
        t1 = time.perf_counter()
        res = col.query(query_embeddings=[qv], n_results=TOPK)
        lats.append((time.perf_counter() - t1) * 1000.0)
        scores: dict[str, float] = {}
        for did, dist in zip(res["ids"][0], res["distances"][0]):
            scores[str(did)] = max(0.0, 1.0 - float(dist))
        run[qid] = scores
    return run, statistics.mean(lats) if lats else 0.0, write_ms


def run_chorus(
    doc_ids: list[str],
    doc_texts: list[str],
    doc_vecs: list[list[float]],
    test_qids: list[str],
    q_texts: list[str],
    q_vecs: list[list[float]],
    qrels: dict[str, dict[str, int]],
) -> dict[str, Any]:
    """CHORUS Lane: batch imprint + full-vector IR recall (≤50k traces)."""
    configure_ir_env()
    brain = open_lane("chorus")
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

    t1 = time.perf_counter()
    batch_hits = brain.chorus_recall_batch(q_texts, q_vecs, limit=TOPK)
    batch_s = time.perf_counter() - t1
    per_query_ms = batch_s / max(len(test_qids), 1) * 1000.0

    single_lats: list[float] = []
    for qtxt, qv in zip(q_texts[:100], q_vecs[:100]):
        tq = time.perf_counter()
        brain.chorus_recall(qtxt, limit=TOPK, semantic_vector=qv)
        single_lats.append((time.perf_counter() - tq) * 1000.0)
    single_p50 = statistics.median(single_lats) if single_lats else per_query_ms

    run: dict[str, dict[str, float]] = {}
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
    return {
        **score_run(run, qrels),
        "imprint_ms_per_doc": round(imprint_s / len(doc_ids) * 1000.0, 3),
        "query_ms": round(single_p50, 2),
        "query_ms_mean": round(per_query_ms, 2),
        "imprint_wall_s": round(imprint_s, 2),
        "query_batch_wall_s": round(batch_s, 2),
        "lane": "chorus_grg_prism",
        "sleep": sleep_report,
    }


def main() -> int:
    ap = argparse.ArgumentParser(description="BEIR certified IR benchmark (Chroma vs CHORUS)")
    ap.add_argument("--json-out", type=Path, default=None)
    ap.add_argument(
        "--skip-chroma",
        action="store_true",
        help="Reuse chroma metrics from --chroma-json",
    )
    ap.add_argument(
        "--chroma-json",
        type=Path,
        default=REPO / "benchmarks" / "results" / "beir-scifact-2026-07-08.json",
        help="Prior chroma result when --skip-chroma",
    )
    args = ap.parse_args()

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
    from chromadb.utils import embedding_functions

    emb = embedding_functions.ONNXMiniLM_L6_V2()
    if cache.is_file():
        cached = pickle.loads(cache.read_bytes())
        doc_vecs = cached["doc_vecs"]
        q_vecs = cached["q_vecs"]
        embed_doc_ms = cached.get("embed_doc_ms", 0.0)
        embed_q_ms = cached.get("embed_q_ms", 0.0)
    else:
        t0 = time.perf_counter()
        doc_vecs = [list(map(float, v)) for v in emb(doc_texts)]
        embed_doc_ms = (time.perf_counter() - t0) / len(doc_ids) * 1000.0
        t0 = time.perf_counter()
        q_vecs = [list(map(float, v)) for v in emb(q_texts)]
        embed_q_ms = (time.perf_counter() - t0) / len(test_qids) * 1000.0
        cache.write_bytes(
            pickle.dumps(
                {
                    "doc_vecs": doc_vecs,
                    "q_vecs": q_vecs,
                    "embed_doc_ms": embed_doc_ms,
                    "embed_q_ms": embed_q_ms,
                }
            )
        )

    if args.skip_chroma and args.chroma_json.is_file():
        prior = json.loads(args.chroma_json.read_text())
        chroma_block = prior.get("chroma", {})
        chroma_q_ms = float(chroma_block.get("query_ms", 0.0))
        chroma_w_ms = float(chroma_block.get("write_ms_per_doc", 0.0))
        chroma_scores = {
            k: float(chroma_block[k])
            for k in ("ndcg_at_10", "recall_at_10", "recall_at_100")
            if k in chroma_block
        }
        print("chroma: reusing prior metrics", flush=True)
    else:
        chroma_run, chroma_q_ms, chroma_w_ms = run_chroma(
            doc_ids, doc_texts, doc_vecs, test_qids, q_vecs
        )
        chroma_scores = score_run(chroma_run, qrels)

    print("fluctlight CHORUS imprint+recall...", flush=True)
    fl_chorus_block = run_chorus(
        doc_ids, doc_texts, doc_vecs, test_qids, q_texts, q_vecs, qrels
    )

    out: dict[str, Any] = {
        "benchmark": "beir",
        "dataset": BEIR_DS,
        "docs": len(doc_ids),
        "queries": len(test_qids),
        "embed_doc_ms": round(embed_doc_ms, 2),
        "embed_query_ms": round(embed_q_ms, 2),
        "chroma": {
            **chroma_scores,
            "write_ms_per_doc": round(chroma_w_ms, 2),
            "query_ms": round(chroma_q_ms, 2),
        },
        "fluctlightdb_chorus": fl_chorus_block,
    }
    print(json.dumps(out, indent=2))
    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(json.dumps(out, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
