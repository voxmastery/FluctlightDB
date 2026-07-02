#!/usr/bin/env python3
"""LongMemEval-S — answer-in-recall benchmark for FluctlightDB (conv / index modes)."""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
import time
import urllib.error
import urllib.request
from collections import defaultdict
from pathlib import Path
from typing import Any, Optional

REPO = Path(__file__).resolve().parents[1]
SDK = REPO / "sdks" / "python"
if SDK.is_dir() and str(SDK) not in sys.path:
    sys.path.insert(0, str(SDK))

from fluctlightdb import connect_conv, connect_index  # noqa: E402

DEFAULT_DATA = Path("/tmp/longmemeval/data/longmemeval_s_cleaned.json")
EMBED_URL = os.environ.get("FLUCTLIGHT_EMBED_URL", "http://127.0.0.1:8793/embed").rstrip("/")


def normalize(text: str) -> str:
    return re.sub(r"\s+", " ", (text or "").lower().strip())


def answer_in_recalls(recalls: list[dict], answer: str, top_k: int = 8) -> bool:
    ans = normalize(answer)
    if not ans:
        return False
    ans_tokens = [t for t in re.findall(r"[a-z0-9]+", ans) if len(t) > 2]
    for r in recalls[:top_k]:
        ep = r.get("episode") or {}
        content = normalize(ep.get("content") or "")
        if not content:
            continue
        if ans in content or content in ans:
            return True
        if ans_tokens and sum(1 for t in ans_tokens if t in content) >= max(1, len(ans_tokens) * 2 // 3):
            return True
    return False


class EmbedCache:
    def __init__(self, url: str):
        self.url = url.rstrip("/")
        self.cache: dict[str, list[float]] = {}

    def embed_many(self, texts: list[str]) -> list[Optional[list[float]]]:
        out: list[Optional[list[float]]] = [None] * len(texts)
        missing_idx: list[int] = []
        missing_texts: list[str] = []
        for i, t in enumerate(texts):
            key = (t or "")[:2000]
            if not key.strip():
                continue
            if key in self.cache:
                out[i] = self.cache[key]
            else:
                missing_idx.append(i)
                missing_texts.append(key)
        if not missing_texts:
            return out
        try:
            req = urllib.request.Request(
                f"{self.url}/embed/batch",
                data=json.dumps({"texts": missing_texts}).encode(),
                headers={"Content-Type": "application/json"},
                method="POST",
            )
            with urllib.request.urlopen(req, timeout=300) as resp:
                data = json.loads(resp.read().decode())
            vecs = data.get("embeddings") or []
            for j, vec in enumerate(vecs):
                if j >= len(missing_idx):
                    break
                if isinstance(vec, list) and vec:
                    v = [float(x) for x in vec]
                    key = missing_texts[j]
                    self.cache[key] = v
                    out[missing_idx[j]] = v
        except Exception:
            for i, t in enumerate(texts):
                if out[i] is None:
                    out[i] = self.embed_one(t)
        return out

    def embed_one(self, text: str) -> Optional[list[float]]:
        key = (text or "")[:2000]
        if not key.strip():
            return None
        if key in self.cache:
            return self.cache[key]
        try:
            req = urllib.request.Request(
                f"{self.url}/embed",
                data=json.dumps({"text": key}).encode(),
                headers={"Content-Type": "application/json"},
                method="POST",
            )
            with urllib.request.urlopen(req, timeout=60) as resp:
                data = json.loads(resp.read().decode())
            vec = data.get("embedding") or data.get("vector")
            if isinstance(vec, list) and vec:
                out = [float(x) for x in vec]
                self.cache[key] = out
                return out
        except Exception:
            return None
        return None

    def embed(self, text: str) -> Optional[list[float]]:
        return self.embed_one(text)


def load_dataset(path: Path) -> list[dict]:
    with path.open() as f:
        data = json.load(f)
    if not isinstance(data, list):
        raise SystemExit(f"expected list in {path}")
    return data


def ingest_item(brain: Any, item: dict, embedder: EmbedCache, *, fast: bool) -> int:
    turns: list[tuple[str, str]] = []
    for session in item.get("haystack_sessions") or []:
        if not isinstance(session, list):
            continue
        for msg in session:
            if not isinstance(msg, dict):
                continue
            role = (msg.get("role") or "user").strip()
            content = (msg.get("content") or "").strip()
            if content:
                turns.append((role, content))
    if not turns:
        return 0
    vectors: list[Optional[list[float]]] = [None] * len(turns)
    if not fast:
        vectors = embedder.embed_many([c for _, c in turns])
    n = 0
    for (role, content), vec in zip(turns, vectors):
        line = f"{role}: {content[:480]}"
        brain.experience(
            line,
            context="longmemeval",
            salience=0.55,
            semantic_vector=vec,
        )
        n += 1
    return n


def eval_one(
    item: dict,
    *,
    mode: str,
    top_k: int,
    embedder: EmbedCache,
    fast: bool,
) -> dict[str, Any]:
    t0 = time.perf_counter()
    if mode == "index":
        brain = connect_index()
    else:
        brain = connect_conv()
    ingested = ingest_item(brain, item, embedder, fast=fast)
    question = (item.get("question") or "").strip()
    qvec = embedder.embed(question) if not fast else None
    act = brain.activate(question, semantic_vector=qvec, limit=top_k)
    recalls = act.get("recalls") or []
    answer = item.get("answer") or ""
    hit = answer_in_recalls(recalls, answer, top_k=top_k)
    elapsed = time.perf_counter() - t0
    return {
        "question_id": item.get("question_id"),
        "question_type": item.get("question_type"),
        "hit": hit,
        "ingested": ingested,
        "recalls": len(recalls),
        "sec": round(elapsed, 3),
    }


def main() -> int:
    ap = argparse.ArgumentParser(description="FluctlightDB LongMemEval-S benchmark")
    ap.add_argument("--data", type=Path, default=DEFAULT_DATA)
    ap.add_argument("--mode", choices=("conv", "index"), default="index")
    ap.add_argument("--top-k", type=int, default=8)
    ap.add_argument("--limit", type=int, default=0, help="0 = full dataset")
    ap.add_argument("--offset", type=int, default=0)
    ap.add_argument("--fast", action="store_true", help="skip embeddings (lexical only)")
    ap.add_argument("--json-out", type=Path, default=None)
    ap.add_argument("--checkpoint", type=Path, default=None, help="resume/save progress JSONL")
    args = ap.parse_args()

    if not args.data.is_file():
        raise SystemExit(f"dataset not found: {args.data}")

    items = load_dataset(args.data)
    if args.offset:
        items = items[args.offset :]
    if args.limit > 0:
        items = items[: args.limit]

    done_ids: set[str] = set()
    prior: list[dict] = []
    if args.checkpoint and args.checkpoint.is_file():
        with args.checkpoint.open() as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                row = json.loads(line)
                prior.append(row)
                qid = row.get("question_id")
                if qid:
                    done_ids.add(str(qid))

    embedder = EmbedCache(EMBED_URL)
    results = list(prior)
    hits = sum(1 for r in results if r.get("hit"))
    t_start = time.perf_counter()

    for i, item in enumerate(items):
        qid = str(item.get("question_id") or i)
        if qid in done_ids:
            continue
        try:
            row = eval_one(
                item,
                mode=args.mode,
                top_k=args.top_k,
                embedder=embedder,
                fast=args.fast,
            )
        except Exception as e:
            row = {
                "question_id": item.get("question_id"),
                "question_type": item.get("question_type"),
                "hit": False,
                "error": str(e)[:200],
                "sec": 0.0,
            }
        results.append(row)
        if row.get("hit"):
            hits += 1
        if args.checkpoint:
            args.checkpoint.parent.mkdir(parents=True, exist_ok=True)
            with args.checkpoint.open("a") as f:
                f.write(json.dumps(row) + "\n")
        n_done = len(results)
        if n_done % 5 == 0 or n_done == len(items) + len(prior):
            rate = hits / n_done if n_done else 0.0
            print(
                f"[{n_done}] answer_in_recall@{args.top_k}={rate:.1%} "
                f"({hits}/{n_done}) last_sec={row.get('sec', 0)}",
                flush=True,
            )

    wall = time.perf_counter() - t_start
    by_type: dict[str, list[bool]] = defaultdict(list)
    for r in results:
        by_type[str(r.get("question_type") or "unknown")].append(bool(r.get("hit")))

    report = {
        "benchmark": "longmemeval_s",
        "dataset": str(args.data),
        "mode": args.mode,
        "top_k": args.top_k,
        "questions": len(results),
        "answer_in_recall_at_k": round(hits / len(results), 4) if results else 0.0,
        "hits": f"{hits}/{len(results)}",
        "wall_s": round(wall, 1),
        "sec_per_question": round(wall / max(1, len(results) - len(prior)), 2),
        "embed_cache_size": len(embedder.cache),
        "by_type": {
            k: round(sum(v) / len(v), 4) for k, v in sorted(by_type.items())
        },
    }
    print(json.dumps(report, indent=2))

    out = args.json_out or REPO / "benchmarks" / "results" / f"longmemeval-{time.strftime('%Y-%m-%d')}.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps({"summary": report, "results": results}, indent=2))
    print(f"Wrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
