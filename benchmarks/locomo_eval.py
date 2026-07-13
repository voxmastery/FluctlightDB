#!/usr/bin/env python3
"""LoCoMo long-dialogue evidence recall benchmark.

Reports BOTH:
  - raw: gold dia_ids in retrieved set only (stricter)
  - expanded: retrieved set plus expand_session_neighbors(±window) (historical default window=3)

The expanded protocol is a FluctlightDB harness choice (not upstream LoCoMo's
exact-match-in-context check). Primary JSON field mean_evidence_recall aliases
expanded for freeze compatibility; always read mean_evidence_recall_raw too.

Usage:
  LOCOMO_DATA=/tmp/locomo10.json \\
  PYTHONPATH=sdks/python python3 benchmarks/locomo_eval.py --mode chorus --top-k 150
  # strict only scoring window:
  PYTHONPATH=sdks/python python3 benchmarks/locomo_eval.py --neighbor-window 0
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
SDK = REPO / "sdks/python"
if str(SDK) not in sys.path:
    sys.path.insert(0, str(SDK))

from bench_lanes import chorus_hits_to_ids, configure_ir_env, open_lane  # noqa: E402
from locomo_metrics import evidence_hit, evidence_recall_fraction, summarize_hits  # noqa: E402

DEFAULT_DATA = Path(os.environ.get("LOCOMO_DATA", "/tmp/locomo/locomo10.json"))
CACHE_DIR = Path(os.environ.get("LOCOMO_CACHE", "/tmp/locomo/cache"))


def batch_embed(texts: list[str], batch_size: int = 128) -> list[list[float]]:
    from chromadb.utils import embedding_functions

    emb = embedding_functions.ONNXMiniLM_L6_V2()
    out: list[list[float]] = []
    for i in range(0, len(texts), batch_size):
        chunk = texts[i : i + batch_size]
        out.extend(list(map(lambda v: list(map(float, v)), emb(chunk))))
    return out


class EmbedCache:
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


def collect_turns(item: dict, rag_mode: str, context_window: int = 1) -> list[dict[str, str]]:
    """Collect dialogue turns with a ±context_window sliding context window.

    Each chunk keeps its own dia_id for evidence matching, but the stored body
    includes adjacent turns from the same session — mimicking how the hippocampus
    always encodes events within their temporal context (episodic binding).
    """
    rows: list[dict[str, str]] = []
    conv = item.get("conversation") or {}
    for sess_key, turns in iter_sessions(conv):
        # First pass: collect all valid turns in this session
        sess_rows: list[dict[str, str]] = []
        for turn in turns:
            if not isinstance(turn, dict):
                continue
            dia = str(turn.get("dia_id") or "").strip()
            text = (turn.get("text") or "").strip()
            if not dia or not text:
                continue
            speaker = turn.get("speaker") or "user"
            sess_rows.append(
                {
                    "dia": dia,
                    "bare": f"[{dia}] {speaker}: {text}",
                    "kind": "turn",
                }
            )
        # Second pass: enrich each turn with ±context_window neighbours (same session)
        for i, row in enumerate(sess_rows):
            lo = max(0, i - context_window)
            hi = min(len(sess_rows), i + context_window + 1)
            # Join neighbouring turn lines; current turn stays central
            ctx_body = "  ".join(sess_rows[j]["bare"] for j in range(lo, hi))
            rows.append(
                {
                    "dia": row["dia"],
                    "body": ctx_body,
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


def ingest_chorus(
    brain: Any,
    item: dict,
    embedder: EmbedCache,
    *,
    rag_mode: str,
) -> int:
    rows = collect_turns(item, rag_mode)
    if not rows:
        return 0
    vecs = embedder.get_many([r["body"][:800] for r in rows])
    batch = [
        {
            "memory_id": row["dia"],
            "content": row["body"],
            "context": f"locomo:{row['dia']}",
            "semantic_vector": vec,
            "salience": 0.72 if row["kind"] == "obs" else 0.62,
        }
        for row, vec in zip(rows, vecs)
    ]
    return int(brain.chorus_imprint_batch(batch))


def ingest_muon(
    brain: Any,
    item: dict,
    *,
    rag_mode: str,
) -> int:
    conv = item.get("conversation") or {}
    batch: list[dict[str, str]] = []
    for sess_key, turns in iter_sessions(conv):
        lines: list[str] = []
        for turn in turns:
            if not isinstance(turn, dict):
                continue
            dia = str(turn.get("dia_id") or "").strip()
            text = (turn.get("text") or "").strip()
            if not dia or not text:
                continue
            speaker = turn.get("speaker") or "user"
            lines.append(f"[{dia}] {speaker}: {text}")
        if lines:
            batch.append(
                {
                    "session_id": sess_key,
                    "date": "",
                    "body": "\n".join(lines),
                    "user_keys": "\n".join(lines)[:4000],
                }
            )
    if rag_mode in ("all", "obs"):
        obs = item.get("observation") or {}
        for sess_key, speakers in obs.items():
            if not isinstance(speakers, dict):
                continue
            obs_lines: list[str] = []
            for _speaker, facts in speakers.items():
                if not isinstance(facts, list):
                    continue
                for row in facts:
                    if not isinstance(row, (list, tuple)) or len(row) < 2:
                        continue
                    fact, dia = str(row[0]), str(row[1])
                    if fact and dia:
                        obs_lines.append(f"[{dia}] {fact}")
            if obs_lines:
                sid = f"{sess_key}:obs"
                batch.append(
                    {
                        "session_id": sid,
                        "date": "",
                        "body": "\n".join(obs_lines),
                        "user_keys": "\n".join(obs_lines)[:4000],
                    }
                )
    if not batch:
        return 0
    brain.muon_imprint_batch(batch)
    return len(batch)


def expand_session_neighbors(found: set[str], item: dict, window: int = 3) -> set[str]:
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


def recalled_dia_ids_from_activate(recalls: list[dict], limit: int) -> set[str]:
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


def recalled_dia_ids_from_chorus(hits: list[Any], limit: int) -> set[str]:
    found: set[str] = set()
    for mid in chorus_hits_to_ids(hits, limit):
        if mid.startswith("D") and ":" in mid:
            found.add(mid)
        found.update(re.findall(r"\bD\d+:\d+\b", mid))
    return found


def recalled_dia_ids_from_muon(hits: list[dict], limit: int) -> set[str]:
    found: set[str] = set()
    for h in hits[:limit]:
        text = str(h.get("snippet") or h.get("content") or "")
        found.update(re.findall(r"\bD\d+:\d+\b", text))
    return found


def normalize_evidence(evidence: list) -> list[str]:
    out: list[str] = []
    for raw in evidence:
        chunk = str(raw).replace(";", " ")
        for tok in chunk.split():
            tok = tok.strip()
            if tok.startswith("D") and ":" in tok:
                out.append(tok)
    return out


def score_evidence(
    found: set[str],
    evidence: list[str],
    item: dict,
    *,
    neighbor_window: int,
) -> dict[str, Any]:
    """Score gold evidence under raw (strict) and optional neighbor-expanded sets."""
    raw = set(found)
    expanded = (
        expand_session_neighbors(raw, item, window=neighbor_window)
        if neighbor_window > 0
        else set(raw)
    )
    return {
        "evidence": evidence,
        "evidence_frac_raw": evidence_recall_fraction(evidence, raw),
        "all_evidence_raw": evidence_hit(evidence, raw),
        "evidence_frac_expanded": evidence_recall_fraction(evidence, expanded),
        "all_evidence_expanded": evidence_hit(evidence, expanded),
        # Legacy aliases = expanded protocol (historical freezes / issue #2 protocol).
        "evidence_frac": evidence_recall_fraction(evidence, expanded),
        "all_evidence": evidence_hit(evidence, expanded),
        "neighbor_window": neighbor_window,
    }


def eval_conversation_chorus(
    brain: Any,
    item: dict,
    embedder: EmbedCache,
    *,
    top_k: int,
    neighbor_window: int = 3,
) -> list[dict]:
    qas = [
        qa
        for qa in (item.get("qa") or [])
        if isinstance(qa, dict) and (qa.get("question") or "").strip() and qa.get("evidence")
    ]
    if not qas:
        return []
    questions = [(qa.get("question") or "").strip()[:400] for qa in qas]
    q_vecs = embedder.get_many(questions)
    batch = brain.chorus_recall_batch(questions, q_vecs, limit=top_k)
    rows: list[dict] = []
    for qa, hits in zip(qas, batch):
        found = recalled_dia_ids_from_chorus(hits, top_k)
        evidence = normalize_evidence(qa.get("evidence") or [])
        scored = score_evidence(
            found, evidence, item, neighbor_window=neighbor_window
        )
        rows.append(
            {
                "question": (qa.get("question") or "").strip(),
                "recall_n": len(hits or []),
                **scored,
            }
        )
    return rows


def eval_conversation_muon(
    brain: Any,
    item: dict,
    *,
    top_k: int,
    neighbor_window: int = 3,
) -> list[dict]:
    qas = [
        qa
        for qa in (item.get("qa") or [])
        if isinstance(qa, dict) and (qa.get("question") or "").strip() and qa.get("evidence")
    ]
    rows: list[dict] = []
    for qa in qas:
        question = (qa.get("question") or "").strip()
        raw = brain.tau_recall(question, limit=top_k)
        if not raw:
            raw = brain.muon_recall(question, limit=top_k)
            if isinstance(raw, dict):
                raw = raw.get("hits") or []
        found = recalled_dia_ids_from_muon(raw or [], top_k)
        evidence = normalize_evidence(qa.get("evidence") or [])
        scored = score_evidence(
            found, evidence, item, neighbor_window=neighbor_window
        )
        rows.append(
            {
                "question": question,
                "recall_n": len(raw or []),
                **scored,
            }
        )
    return rows


def main() -> int:
    ap = argparse.ArgumentParser(description="LoCoMo evidence recall benchmark")
    ap.add_argument("--data", type=Path, default=DEFAULT_DATA)
    ap.add_argument(
        "--mode",
        choices=("chorus", "muon"),
        default=os.environ.get("LOCOMO_MODE", "chorus"),
    )
    ap.add_argument("--rag-mode", choices=("dialog", "obs", "all"), default="all")
    ap.add_argument("--top-k", type=int, default=int(os.environ.get("LOCOMO_TOP_K", "150")))
    ap.add_argument(
        "--neighbor-window",
        type=int,
        default=int(os.environ.get("LOCOMO_NEIGHBOR_WINDOW", "3")),
        help=(
            "Expand each retrieved dia_id by ±N session neighbors before scoring "
            "(historical default 3). Use 0 for strict raw evidence recall. "
            "Both raw and expanded are always reported."
        ),
    )
    ap.add_argument("--limit", type=int, default=0, help="0 = all conversations")
    ap.add_argument("--json-out", type=Path, default=None)
    args = ap.parse_args()

    configure_ir_env()
    items = load_locomo(args.data)
    if args.limit > 0:
        items = items[: args.limit]

    cache_path = CACHE_DIR / "minilm_vecs.pkl"
    embedder = EmbedCache(cache_path)
    t0 = time.perf_counter()
    all_rows: list[dict] = []
    total_ingest = 0

    for item in items:
        brain = open_lane(args.mode)
        if args.mode == "chorus":
            total_ingest += ingest_chorus(brain, item, embedder, rag_mode=args.rag_mode)
            all_rows.extend(
                eval_conversation_chorus(
                    brain,
                    item,
                    embedder,
                    top_k=args.top_k,
                    neighbor_window=args.neighbor_window,
                )
            )
        else:
            total_ingest += ingest_muon(brain, item, rag_mode=args.rag_mode)
            all_rows.extend(
                eval_conversation_muon(
                    brain,
                    item,
                    top_k=args.top_k,
                    neighbor_window=args.neighbor_window,
                )
            )

    embedder.save()

    summary = summarize_hits(all_rows)
    lane = "chorus_grg" if args.mode == "chorus" else "muon_tau"
    out = {
        "benchmark": "locomo",
        "dataset": str(args.data),
        "mode": args.mode,
        "lane": lane,
        "rag_mode": args.rag_mode,
        "top_k": args.top_k,
        "neighbor_window": args.neighbor_window,
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