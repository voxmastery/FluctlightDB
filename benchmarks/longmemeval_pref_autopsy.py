"""Dump gold sessions + ranked recalls for one LongMemEval preference miss."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from longmemeval_bench import (
    EmbedCache,
    expand_queries,
    load_dataset,
    retrieve_item,
    session_ids_from_recalls,
)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", type=Path, required=True)
    ap.add_argument("--qid", default="95228167")
    ap.add_argument("--top-k", type=int, default=8)
    ap.add_argument("--fast", action="store_true")
    args = ap.parse_args()
    items = [it for it in load_dataset(args.data) if str(it.get("question_id")) == args.qid]
    if not items:
        raise SystemExit(f"qid not found: {args.qid}")
    item = items[0]
    embedder = EmbedCache()
    recalls, hit, ingested, _ = retrieve_item(
        item,
        mode="index",
        top_k=args.top_k,
        embedder=embedder,
        fast=args.fast,
        granularity="session",
        query_expand=True,
        dual_key=True,
        pref_facts_key=True,
    )
    ranked = session_ids_from_recalls(recalls, top_k=args.top_k)
    gold = [str(x) for x in (item.get("answer_session_ids") or [])]
    out = {
        "question_id": args.qid,
        "question": item.get("question"),
        "question_type": item.get("question_type"),
        "gold_session_ids": gold,
        "expand_queries": expand_queries(item.get("question") or "", item.get("question_type")),
        "hit_at_k": hit,
        "ranked_session_ids": ranked,
        "gold_rank": min((ranked.index(g) for g in gold if g in ranked), default=None),
        "ingested": ingested,
        "n_recalls": len(recalls),
    }
    print(json.dumps(out, indent=2))
    return 0 if hit else 1


if __name__ == "__main__":
    raise SystemExit(main())
