#!/usr/bin/env python3
"""BEAM (ICLR 2026) retrieval-layer evaluation for FluctlightDB.

Full BEAM E2E uses LLM rubric grading (see upstream BEAM repo). This harness
measures **context recall**: whether probing-question evidence turns appear in
CHORUS top-k — comparable to LoCoMo evidence recall, not LLM-judge QA.

Data: https://github.com/mohammadtavakoli78/BEAM
Paper: https://arxiv.org/abs/2510.27246

Usage:
  PYTHONPATH=sdks/python python benchmarks/beam_eval.py --smoke
  PYTHONPATH=sdks/python python benchmarks/beam_eval.py --chat-id 1 --size 100K
"""

from __future__ import annotations

import argparse
import json
import os
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
from locomo_eval import EmbedCache, batch_embed  # noqa: E402

BEAM_RAW = "https://raw.githubusercontent.com/mohammadtavakoli78/BEAM/main"
DEFAULT_CACHE = Path(os.environ.get("BEAM_CACHE", "/tmp/beam"))


def download(url: str, dest: Path) -> None:
    dest.parent.mkdir(parents=True, exist_ok=True)
    if dest.is_file():
        return
    import urllib.request

    print(f"==> download {url}")
    urllib.request.urlretrieve(url, dest)


def load_chat(size: str, chat_id: str, cache: Path) -> list[dict]:
    base = cache / "chats" / size / chat_id
    chat_path = base / "chat.json"
    download(f"{BEAM_RAW}/chats/{size}/{chat_id}/chat.json", chat_path)
    with chat_path.open() as f:
        return json.load(f)


def load_probes(size: str, chat_id: str, cache: Path) -> dict[str, list[dict]]:
    path = cache / "chats" / size / chat_id / "probing_questions" / "probing_questions.json"
    download(
        f"{BEAM_RAW}/chats/{size}/{chat_id}/probing_questions/probing_questions.json",
        path,
    )
    with path.open() as f:
        return json.load(f)


def iter_turns(batches: list[dict]) -> list[dict[str, str]]:
    rows: list[dict[str, str]] = []
    for batch in batches:
        for turn_group in batch.get("turns") or []:
            for turn in turn_group:
                if not isinstance(turn, dict):
                    continue
                content = (turn.get("content") or "").strip()
                if not content:
                    continue
                idx = turn.get("id")
                if idx is None:
                    idx = turn.get("index")
                if idx is None:
                    idx = len(rows)
                memory_id = str(idx)
                role = turn.get("role") or "user"
                rows.append(
                    {
                        "memory_id": memory_id,
                        "body": f"[{memory_id}] {role}: {content[:1200]}",
                        "context": f"beam:{memory_id}",
                    }
                )
    return rows


def gold_turn_ids(question: dict) -> list[str]:
    ids: list[str] = []
    for key in ("conversation_references",):
        for ref in question.get(key) or []:
            m = re.search(r"chat_id:\s*(\d+)", str(ref))
            if m:
                ids.append(m.group(1))
    src = question.get("source_chat_ids")
    if isinstance(src, dict):
        for vals in src.values():
            for v in vals or []:
                ids.append(str(v))
    return ids


def rubric_terms(question: dict) -> list[str]:
    terms: list[str] = []
    for key in ("rubric", "ideal_answer", "ideal_response"):
        val = question.get(key)
        if isinstance(val, list):
            terms.extend(str(x) for x in val if x)
        elif isinstance(val, str) and val.strip():
            terms.append(val.strip())
    # Short discriminative phrases only
    out: list[str] = []
    for t in terms:
        t = re.sub(r"\s+", " ", t).strip()
        if len(t) >= 24:
            out.append(t[:200].lower())
    return out[:3]


def context_hit(
    recalled_text: str,
    recalled_ids: list[str],
    question: dict,
    qtype: str,
) -> bool:
    recalled_lower = recalled_text.lower()
    for tid in gold_turn_ids(question):
        if tid in recalled_ids:
            return True
    if qtype == "abstention":
        return True  # retrieval metric N/A; counted separately
    for term in rubric_terms(question):
        if term[:40] in recalled_lower:
            return True
    return False


def eval_chat(
    brain: Any,
    embedder: EmbedCache,
    batches: list[dict],
    probes: dict[str, list[dict]],
    *,
    top_k: int,
) -> dict[str, Any]:
    rows = iter_turns(batches)
    vecs = embedder.get_many([r["body"][:800] for r in rows])
    batch = [
        {
            "memory_id": row["memory_id"],
            "content": row["body"],
            "context": row["context"],
            "semantic_vector": vec,
            "salience": 0.62,
        }
        for row, vec in zip(rows, vecs)
    ]
    imprinted = int(brain.chorus_imprint_batch(batch))

    hits = 0
    scored = 0
    by_type: dict[str, list[bool]] = {}

    for qtype, questions in probes.items():
        for q in questions:
            if qtype == "abstention":
                continue
            scored += 1
            cue = str(q.get("question") or "")
            recall = brain.chorus_recall(cue, limit=top_k)
            ids = chorus_hits_to_ids(recall, top_k)
            snippets: list[str] = []
            for h in recall[:top_k]:
                if isinstance(h, dict):
                    snippets.append(str(h.get("snippet") or h.get("memory_id") or ""))
                elif isinstance(h, (list, tuple)) and len(h) >= 1:
                    snippets.append(str(h[0]))
            recalled_text = "\n".join(snippets)
            ok = context_hit(recalled_text, ids, q, qtype)
            by_type.setdefault(qtype, []).append(ok)
            if ok:
                hits += 1

    embedder.save()
    return {
        "turns_ingested": len(rows),
        "memories_imprinted": imprinted,
        "questions_scored": scored,
        "context_hits": hits,
        "context_recall": (hits / scored) if scored else 0.0,
        "by_type": {k: sum(v) / len(v) for k, v in by_type.items()},
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="BEAM retrieval-layer eval (CHORUS)")
    parser.add_argument("--size", default="100K", help="BEAM chat size folder (100K, 500K, ...)")
    parser.add_argument("--chat-id", default="1", help="Chat id under chats/<size>/")
    parser.add_argument("--top-k", type=int, default=32)
    parser.add_argument("--smoke", action="store_true", help="Same as --size 100K --chat-id 1")
    parser.add_argument("--json-out", type=Path, default=None)
    args = parser.parse_args()

    if args.smoke:
        args.size = "100K"
        args.chat_id = "1"

    configure_ir_env()
    cache = DEFAULT_CACHE
    batches = load_chat(args.size, args.chat_id, cache)
    probes = load_probes(args.size, args.chat_id, cache)

    embed_cache = EmbedCache(cache / "embed_cache.pkl")
    brain = open_lane("chorus")

    t0 = time.time()
    result = eval_chat(brain, embed_cache, batches, probes, top_k=args.top_k)
    wall = time.time() - t0

    out = {
        "benchmark": "beam",
        "lane": "chorus_grg",
        "metric": "context_recall",
        "note": "Retrieval-layer only; not BEAM LLM rubric score",
        "beam_upstream": "https://github.com/mohammadtavakoli78/BEAM",
        "chat_size": args.size,
        "chat_id": args.chat_id,
        "top_k": args.top_k,
        "wall_s": round(wall, 2),
        **result,
    }

    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(json.dumps(out, indent=2) + "\n")
        print(f"wrote {args.json_out}")

    print(json.dumps(out, indent=2))
    print(
        f"BEAM context recall @ {args.top_k}: {out['context_hits']}/{out['questions_scored']} "
        f"({out['context_recall']:.1%})"
    )


if __name__ == "__main__":
    main()
