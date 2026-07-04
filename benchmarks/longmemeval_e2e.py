#!/usr/bin/env python3
"""LongMemEval-S end-to-end: FluctlightDB retrieve → reader LLM → official GPT judge."""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
from collections import defaultdict
from pathlib import Path
from typing import Any, Optional

REPO = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(REPO / "benchmarks"))
sys.path.insert(0, str(REPO / "sdks" / "python"))

from longmemeval_bench import (  # noqa: E402
    DEFAULT_DATA,
    EmbedCache,
    load_dataset,
    retrieve_item,
    session_ids_from_recalls,
)
from prompts.longmemeval_judge import get_anscheck_prompt  # noqa: E402

READER_TEMPLATE = (
    "I will give you several history chats between you and a user. "
    "Please answer the question based on the relevant chat history.\n\n\n"
    "History Chats:\n\n{history}\n\nCurrent Date: {question_date}\n"
    "Question: {question}\nAnswer:"
)

from cursor_api import cursor_auto_chat, load_cursor_api_key  # noqa: E402

READER_MODEL = "auto"
JUDGE_MODEL = "auto"


def format_history_json(item: dict, session_ids: list[str]) -> str:
    id2idx = {
        str(sid): i for i, sid in enumerate(item.get("haystack_session_ids") or [])
    }
    dates: list[str] = list(item.get("haystack_dates") or [])
    sessions = item.get("haystack_sessions") or []
    chunks: list[tuple[str, list[dict]]] = []
    for sid in session_ids:
        idx = id2idx.get(str(sid))
        if idx is None or idx >= len(sessions):
            continue
        date = dates[idx] if idx < len(dates) else ""
        cleaned: list[dict] = []
        for turn in sessions[idx]:
            if not isinstance(turn, dict):
                continue
            t = {k: v for k, v in turn.items() if k != "has_answer"}
            cleaned.append(t)
        chunks.append((date, cleaned))
    chunks.sort(key=lambda x: x[0])
    parts: list[str] = []
    for i, (date, sess) in enumerate(chunks):
        parts.append(
            f"\n### Session {i + 1}:\nSession Date: {date}\nSession Content:\n"
            + json.dumps(sess)
        )
    return "".join(parts)


def llm_chat(prompt: str, *, model: str, timeout_s: int = 300) -> str:
    return cursor_auto_chat(prompt, model=model or "auto", timeout_s=timeout_s)


def judge_label(
    item: dict,
    hypothesis: str,
    *,
    judge_model: str,
    cursor_timeout_s: int = 120,
) -> bool:
    qtype = str(item.get("question_type") or "")
    qid = str(item.get("question_id") or "")
    prompt = get_anscheck_prompt(
        qtype,
        item.get("question") or "",
        item.get("answer") or "",
        hypothesis,
        abstention="_abs" in qid,
    )
    resp = llm_chat(prompt, model=judge_model, timeout_s=cursor_timeout_s)
    return "yes" in resp.lower()


def aggregate_qa(rows: list[dict]) -> dict[str, Any]:
    judged = [r for r in rows if r.get("autoeval_label") is not None]
    by_type: dict[str, list[bool]] = defaultdict(list)
    for r in judged:
        by_type[str(r.get("question_type") or "unknown")].append(bool(r.get("autoeval_label")))
    overall = sum(1 for r in judged if r.get("autoeval_label")) / len(judged) if judged else 0.0
    task_avg = (
        sum(sum(v) / len(v) for v in by_type.values() if v) / len(by_type) if by_type else 0.0
    )
    retr = [r for r in rows if r.get("session_recall_hit") is not None]
    retr_rate = sum(1 for r in retr if r.get("session_recall_hit")) / len(retr) if retr else 0.0
    return {
        "overall_accuracy": round(overall, 4),
        "task_averaged_accuracy": round(task_avg, 4),
        "session_recall_at_k": round(retr_rate, 4),
        "judged_n": len(judged),
        "by_type_accuracy": {k: round(sum(v) / len(v), 4) for k, v in sorted(by_type.items())},
    }


def main() -> int:
    ap = argparse.ArgumentParser(description="LongMemEval-S end-to-end QA harness")
    ap.add_argument("--data", type=Path, default=DEFAULT_DATA)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--offset", type=int, default=0)
    ap.add_argument("--top-k", type=int, default=8)
    ap.add_argument("--reader-model", default=READER_MODEL)
    ap.add_argument("--judge-model", default=JUDGE_MODEL)
    ap.add_argument(
        "--cursor-timeout",
        type=int,
        default=int(os.environ.get("CURSOR_API_TIMEOUT", "300")),
        help="seconds per Cursor Cloud Agent prompt (reader/judge)",
    )
    ap.add_argument("--granularity", default="session", choices=("session", "turn"))
    ap.add_argument("--dual-key", action="store_true")
    ap.add_argument("--pref-facts-key", action="store_true")
    ap.add_argument("--query-expand", action="store_true")
    ap.add_argument("--fast", action="store_true", help="lexical retrieval only")
    ap.add_argument("--skip-llm", action="store_true", help="retrieval + prompt only")
    ap.add_argument("--json-out", type=Path, default=None)
    ap.add_argument("--checkpoint", type=Path, default=None)
    ap.add_argument(
        "--type-filter",
        default="",
        help="comma-separated question_type filter",
    )
    args = ap.parse_args()

    if not args.data.is_file():
        raise SystemExit(f"dataset not found: {args.data}")

    items = load_dataset(args.data)
    if args.type_filter.strip():
        allowed = {t.strip() for t in args.type_filter.split(",") if t.strip()}
        items = [it for it in items if it.get("question_type") in allowed]
    if args.offset:
        items = items[args.offset :]
    if args.limit > 0:
        items = items[: args.limit]

    done: set[str] = set()
    rows: list[dict] = []
    if args.checkpoint and args.checkpoint.is_file():
        for line in args.checkpoint.read_text().splitlines():
            if not line.strip():
                continue
            row = json.loads(line)
            rows.append(row)
            qid = row.get("question_id")
            if qid:
                done.add(str(qid))

    embedder = EmbedCache()
    reader_model = args.reader_model
    judge_model = args.judge_model

    if not args.skip_llm and not load_cursor_api_key():
        raise SystemExit(
            "Set CURSOR_API_KEY (serverbrain .env or Colab Secrets), or pass --skip-llm."
        )

    t0 = time.perf_counter()
    for i, item in enumerate(items):
        qid = str(item.get("question_id") or i)
        if qid in done:
            continue
        t_q = time.perf_counter()
        recalls, session_hit, ingested = retrieve_item(
            item,
            mode="index",
            top_k=args.top_k,
            embedder=embedder,
            fast=args.fast,
            granularity=args.granularity,
            query_expand=args.query_expand,
            dual_key=args.dual_key,
            pref_facts_key=args.pref_facts_key,
        )
        sids = session_ids_from_recalls(recalls, top_k=args.top_k)
        history = format_history_json(item, sids)
        reader_prompt = READER_TEMPLATE.format(
            history=history,
            question_date=item.get("question_date") or "",
            question=item.get("question") or "",
        )
        hypothesis = ""
        label: Optional[bool] = None
        if not args.skip_llm:
            hypothesis = llm_chat(
                reader_prompt,
                model=reader_model,
                timeout_s=args.cursor_timeout,
            )
            label = judge_label(
                item,
                hypothesis,
                judge_model=judge_model,
                cursor_timeout_s=min(120, args.cursor_timeout),
            )
        row = {
            "question_id": item.get("question_id"),
            "question_type": item.get("question_type"),
            "session_recall_hit": session_hit,
            "retrieved_sessions": sids,
            "ingested": ingested,
            "hypothesis": hypothesis,
            "autoeval_label": label,
            "reader_model": reader_model if not args.skip_llm else None,
            "judge_model": judge_model if not args.skip_llm else None,
            "llm_backend": "cursor_api" if not args.skip_llm else None,
            "sec": round(time.perf_counter() - t_q, 3),
        }
        rows.append(row)
        if args.checkpoint:
            args.checkpoint.parent.mkdir(parents=True, exist_ok=True)
            with args.checkpoint.open("a") as f:
                f.write(json.dumps(row) + "\n")
        n = len(rows)
        if n % 5 == 0 or (i + 1) == len(items):
            agg = aggregate_qa(rows)
            print(
                f"[{n}] session@k={agg['session_recall_at_k']:.1%} "
                f"e2e={agg['overall_accuracy']:.1%} last_sec={row['sec']}",
                flush=True,
            )

    wall = time.perf_counter() - t0
    summary = {
        "benchmark": "longmemeval_s_e2e",
        "harness": "v4",
        "dataset": str(args.data),
        "granularity": args.granularity,
        "top_k": args.top_k,
        "dual_key": args.dual_key,
        "pref_facts_key": args.pref_facts_key,
        "query_expand": args.query_expand,
        "reader_model": reader_model,
        "judge_model": judge_model,
        "llm_backend": "cursor_api",
        "cursor_api_base": os.environ.get("CURSOR_API_BASE", "https://api.cursor.com"),
        "skip_llm": args.skip_llm,
        "questions": len(rows),
        "wall_s": round(wall, 1),
        **aggregate_qa(rows),
    }
    out = args.json_out or REPO / "benchmarks/results/longmemeval-e2e-v4.json"
    payload = {"summary": summary, "results": rows}
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(payload, indent=2))
    print(json.dumps(summary, indent=2))
    print(f"Wrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
