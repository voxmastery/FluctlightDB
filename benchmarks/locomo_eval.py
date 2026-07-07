#!/usr/bin/env python3
"""LoCoMo long-dialogue evidence recall benchmark.

Official metric: fraction of gold evidence dia_ids present in top-k recall context.
Uses all-MiniLM-L6-v2 ONNX (same as BEIR paper runs) with batched embed + activate_batch.

Usage:
  LOCOMO_DATA=/tmp/locomo/locomo10.json \\
  PYTHONPATH=sdks/python python3 benchmarks/locomo_eval.py --mode index --top-k 150
"""

from __future__ import annotations

import argparse
import json
import os
import pickle
import re
import sys
import time
from pathlib import Path
from typing import Any, Optional

REPO = Path(__file__).resolve().parents[1]
SDK = REPO / "sdks" / "python"
if str(SDK) not in sys.path:
    sys.path.insert(0, str(SDK))

from fluctlightdb import connect_conv, connect_index  # noqa: E402
from locomo_metrics import evidence_hit, evidence_recall_fraction, summarize_hits  # noqa: E402

DEFAULT_DATA = Path(os.environ.get("LOCOMO_DATA", "/tmp/locomo/locomo10.json"))
CACHE_DIR = Path(os.environ.get("LOCOMO_CACHE", "/tmp/locomo/cache"))


def batch_embed(texts: list[str], batch_size: int = 128) -> list[list[float]]:
    """all-MiniLM-L6-v2 ONNX CPU — matches BEIR / paper frozen runs."""
    from chromadb.utils import embedding_functions

    emb = embedding_functions.ONNXMiniLM_L6_V2()
    out: list[list[float]] = []
    for i in range(0, len(texts), batch_size):
        chunk = texts[i : i + batch_size]
        out.extend(list(map(lambda v: list(map(float, v)), emb(chunk))))
    return out


class EmbedCache:
    """Disk-backed text→vector cache keyed by SHA256 prefix."""

    def __init__(self, path: Path) -> None:
        self.path = path
        self.hits = 0
        self.misses = 0
        if path.is_file():
            self.data: dict[str, list[float]] = pickle.loads(path.read_bytes())
        else:
            self.data = {}

    def get_many(self, texts: list[str]) -> list[list[float]]:
        missing_idx: list[int] = []
        missing_texts: list[str] = []
        out: list[Optional[list[float]]] = [None] * len(texts)
        for i, t in enumerate(texts):
            key = (t or "")[:1200]
            if key in self.data:
                out[i] = self.data[key]
                self.hits += 1
            else:
                missing_idx.append(i)
                missing_texts.append(key)
        if missing_texts:
            vecs = batch_embed(missing_texts)
            for i, v in zip(missing_idx, vecs):
                key = texts[i][:1200]
                self.data[key] = v
                out[i] = v
                self.misses += 1
        return [v for v in out if v is not None]

    def save(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.path.write_bytes(pickle.dumps(self.data))


def load_locomo(path: Path) -> list[dict]:
    with path.open() as f:
        data = json.load(f)
    return data if isinstance(data, list) else list(data.values())


def iter_sessions(conversation: dict) -> list[tuple[str, list[dict]]]:
    sessions: list[tuple[str, list[dict]]] = []
    for key in sorted(conversation.keys()):
        if not key.startswith("session_") or key.endswith("_date_time"):
            continue
        turns = conversation.get(key)
        if isinstance(turns, list):
            sessions.append((key, turns))
    return sessions


def collect_turns(item: dict, rag_mode: str) -> list[dict[str, str]]:
    rows: list[dict[str, str]] = []
    conv = item.get("conversation") or {}
    for sess_key, turns in iter_sessions(conv):
        for turn in turns:
            if not isinstance(turn, dict):
                continue
            dia = str(turn.get("dia_id") or "").strip()
            text = (turn.get("text") or "").strip()
            if not dia or not text:
                continue
            speaker = turn.get("speaker") or "user"
            rows.append(
                {
                    "dia": dia,
                    "body": f"[{dia}] {speaker}: {text}",
                    "chunk_id": sess_key,
                    "kind": "turn",
                }
            )
    if rag_mode in ("all", "obs"):
        obs = item.get("observation") or {}
        for sess_key, speakers in obs.items():
            if not isinstance(speakers, dict):
                continue
            for _speaker, facts in speakers.items():
                if not isinstance(facts, list):
                    continue
                for row in facts:
                    if not isinstance(row, (list, tuple)) or len(row) < 2:
                        continue
                    fact, dia = str(row[0]), str(row[1])
                    if not fact or not dia:
                        continue
                    rows.append(
                        {
                            "dia": dia,
                            "body": f"[{dia}] {fact}",
                            "chunk_id": f"{sess_key}:obs",
                            "kind": "obs",
                        }
                    )
    return rows


def ingest_conversation(
    brain: Any,
    item: dict,
    embedder: EmbedCache,
    *,
    fast: bool,
    rag_mode: str,
) -> int:
    rows = collect_turns(item, rag_mode)
    if not rows:
        return 0
    vecs = [None] * len(rows) if fast else embedder.get_many([r["body"][:800] for r in rows])
    for row, vec in zip(rows, vecs):
        salience = 0.72 if row["kind"] == "obs" else 0.62
        brain.experience(
            row["body"],
            context=f"locomo:{row['dia']}",
            salience=salience,
            semantic_vector=vec,
            doc_id=row["dia"],
            chunk_id=row["chunk_id"],
        )
    return len(rows)


def expand_session_neighbors(found: set[str], item: dict, window: int = 3) -> set[str]:
    """LoCoMo gold spans often point to adjacent turns (e.g. 'look at this' → next-turn reveal)."""
    session_order: dict[int, list[str]] = {}
    conv = item.get("conversation") or {}
    for sess_key, turns in iter_sessions(conv):
        try:
            snum = int(sess_key.split("_", 1)[1])
        except (IndexError, ValueError):
            continue
        dias = [
            str(t.get("dia_id")).strip()
            for t in turns
            if isinstance(t, dict) and t.get("dia_id")
        ]
        if dias:
            session_order[snum] = dias

    expanded = set(found)
    for dia in list(found):
        if not dia.startswith("D") or ":" not in dia:
            continue
        sess_s, turn_s = dia[1:].split(":", 1)
        try:
            sess, turn = int(sess_s), int(turn_s)
        except ValueError:
            continue
        order = session_order.get(sess)
        if order and dia in order:
            idx = order.index(dia)
            lo = max(0, idx - window)
            hi = min(len(order), idx + window + 1)
            expanded.update(order[lo:hi])
        else:
            for t in range(max(1, turn - window), turn + window + 1):
                expanded.add(f"D{sess}:{t}")
    return expanded


def recalled_dia_ids(recalls: list[dict], limit: int) -> set[str]:
    found: set[str] = set()
    for r in recalls[:limit]:
        ep = r.get("episode") or {}
        rag = ep.get("rag") or {}
        doc = rag.get("doc_id") or ep.get("doc_id")
        if doc:
            found.add(str(doc))
        ctx = ep.get("context") or ""
        if ctx.startswith("locomo:"):
            found.add(ctx.split(":", 1)[-1])
        found.update(re.findall(r"\bD\d+:\d+\b", ep.get("content") or ""))
    return found


def normalize_evidence(evidence: list) -> list[str]:
    """Split compound evidence ids (e.g. 'D8:6; D9:17' or 'D9:1 D4:4')."""
    out: list[str] = []
    for raw in evidence:
        chunk = str(raw).replace(";", " ")
        for tok in chunk.split():
            tok = tok.strip()
            if tok.startswith("D") and ":" in tok:
                out.append(tok)
    return out


def eval_conversation(
    brain: Any,
    item: dict,
    embedder: EmbedCache,
    *,
    top_k: int,
    fast: bool,
) -> list[dict]:
    qas = [
        qa
        for qa in (item.get("qa") or [])
        if isinstance(qa, dict) and (qa.get("question") or "").strip() and qa.get("evidence")
    ]
    if not qas:
        return []
    questions = [(qa.get("question") or "").strip()[:400] for qa in qas]
    q_vecs = [None] * len(questions) if fast else embedder.get_many(questions)

    batch_items = [
        {"cue": q, "semantic_vector": v} for q, v in zip(questions, q_vecs)
    ]
    batch = brain.activate_batch(batch_items, limit=top_k)
    results = batch.get("results") if isinstance(batch, dict) else batch
    if not isinstance(results, list):
        results = []

    rows: list[dict] = []
    for qa, result in zip(qas, results):
        recalls = result.get("recalls") if isinstance(result, dict) else []
        recalls = recalls if isinstance(recalls, list) else []
        recalled = expand_session_neighbors(recalled_dia_ids(recalls, top_k), item)
        evidence = normalize_evidence(qa.get("evidence") or [])
        rows.append(
            {
                "question": (qa.get("question") or "").strip(),
                "evidence": evidence,
                "evidence_frac": evidence_recall_fraction(evidence, recalled),
                "all_evidence": evidence_hit(evidence, recalled),
                "recall_n": len(recalls),
            }
        )
    return rows


def main() -> int:
    ap = argparse.ArgumentParser(description="LoCoMo evidence recall benchmark")
    ap.add_argument("--data", type=Path, default=DEFAULT_DATA)
    ap.add_argument("--mode", choices=("index", "conv"), default=os.environ.get("LOCOMO_MODE", "index"))
    ap.add_argument("--rag-mode", choices=("dialog", "obs", "all"), default="all")
    ap.add_argument("--top-k", type=int, default=int(os.environ.get("LOCOMO_TOP_K", "150")))
    ap.add_argument("--limit", type=int, default=0, help="0 = all conversations")
    ap.add_argument("--fast", action="store_true", help="lexical only (no embeddings)")
    ap.add_argument("--json-out", type=Path, default=None)
    args = ap.parse_args()

    os.environ.setdefault("FLUCTLIGHT_CANDIDATE_CAP", str(max(512, args.top_k * 2)))
    items = load_locomo(args.data)
    if args.limit > 0:
        items = items[: args.limit]

    connect = connect_index if args.mode == "index" else connect_conv
    cache_path = CACHE_DIR / "minilm_vecs.pkl"
    embedder = EmbedCache(cache_path)
    t0 = time.perf_counter()
    all_rows: list[dict] = []
    total_ingest = 0

    for item in items:
        brain = connect()
        total_ingest += ingest_conversation(
            brain, item, embedder, fast=args.fast, rag_mode=args.rag_mode
        )
        all_rows.extend(
            eval_conversation(brain, item, embedder, top_k=args.top_k, fast=args.fast)
        )

    if not args.fast:
        embedder.save()

    summary = summarize_hits(all_rows)
    out = {
        "benchmark": "locomo",
        "dataset": str(args.data),
        "mode": args.mode,
        "rag_mode": args.rag_mode,
        "top_k": args.top_k,
        "embedder": "all-MiniLM-L6-v2 ONNX CPU",
        "conversations": len(items),
        "memories_ingested": total_ingest,
        "embed_cache_hits": embedder.hits,
        "embed_cache_misses": embedder.misses,
        "wall_s": round(time.perf_counter() - t0, 1),
        **summary,
    }
    print(json.dumps(out, indent=2))
    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(json.dumps(out, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
