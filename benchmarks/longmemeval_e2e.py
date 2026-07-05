#!/usr/bin/env python3
"""LongMemEval-S end-to-end: FluctlightDB retrieve → reader LLM → official GPT judge."""

from __future__ import annotations

import argparse
import json
import os
import sys
import threading
import time
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor, as_completed
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
    session_in_recalls,
)
from prompts.longmemeval_judge import get_anscheck_prompt  # noqa: E402

READER_TEMPLATE = (
    "I will give you several history chats between you and a user. "
    "Please answer the question based on the relevant chat history.\n\n\n"
    "History Chats:\n\n{history}\n\nCurrent Date: {question_date}\n"
    "Question: {question}\nAnswer:"
)

READER_TEMPLATE_COT = (
    "I will give you several history chats between you and a user. "
    "Please answer the question based on the relevant chat history. "
    "Answer the question step by step: first extract all the relevant information, "
    "and then reason over the information to get the answer.\n\n\n"
    "History Chats:\n\n{history}\n\nCurrent Date: {question_date}\n"
    "Question: {question}\nAnswer (step by step):"
)

# OpenAI profiles: standard / max (Mem0-class reader) / brain (CLS + CoN + completion).
E2E_PROFILES: dict[str, dict[str, Any]] = {
    "standard": {
        "reader_model_openai": "gpt-4o-2024-08-06",
        "judge_model_openai": "gpt-4o-2024-08-06",
        "reader_cot": False,
        "reader_con": False,
        "reader_top_k": 8,
        "reader_max_tokens": 1024,
        "judge_max_tokens": 10,
        "bench_mode": "index",
        "brain_sleep": 0,
    },
    "max": {
        "reader_model_openai": "gpt-5",
        "judge_model_openai": "gpt-4o-2024-08-06",
        "reader_cot": True,
        "reader_con": False,
        "reader_top_k": 50,
        "reader_max_tokens": 2048,
        "judge_max_tokens": 10,
        "bench_mode": "index",
        "brain_sleep": 0,
    },
    "brain": {
        "reader_model_openai": "gpt-5",
        "judge_model_openai": "gpt-4o-2024-08-06",
        "reader_cot": False,
        "reader_con": True,
        "reader_top_k": 200,
        "reader_max_tokens": 2048,
        "judge_max_tokens": 10,
        "bench_mode": "brain",
        "brain_sleep": 2,
    },
}

from cursor_api import cursor_auto_chat, load_cursor_api_key  # noqa: E402
from cloud_llm import chat as cloud_chat, load_env_file, smoke_test  # noqa: E402

READER_MODEL = "gemini-2.5-flash"
JUDGE_MODEL = "gemini-2.5-flash"
DEFAULT_LLM_BACKEND = "gemini"


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


def llm_chat(
    prompt: str,
    *,
    backend: str,
    model: str,
    timeout_s: int = 120,
    max_tokens: int = 512,
) -> str:
    if backend == "cursor":
        return cursor_auto_chat(prompt, model=model or "auto", timeout_s=timeout_s)
    return cloud_chat(
        prompt,
        provider=backend,
        model=model or None,
        max_tokens=max_tokens,
        timeout_s=timeout_s,
    )


def judge_label(
    item: dict,
    hypothesis: str,
    *,
    backend: str,
    judge_model: str,
    timeout_s: int = 120,
    max_tokens: int = 10,
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
    resp = llm_chat(
        prompt, backend=backend, model=judge_model, timeout_s=timeout_s, max_tokens=max_tokens
    )
    return resp.strip().lower().startswith("yes") or resp.strip().lower() == "yes"


def process_one_item(
    item: dict,
    *,
    args: argparse.Namespace,
    embedder: EmbedCache,
    reader_model: str,
    judge_model: str,
) -> dict:
    t_q = time.perf_counter()
    recall_k = max(args.top_k, args.reader_top_k)
    recalls, _, ingested, brain = retrieve_item(
        item,
        mode=args.bench_mode,
        top_k=recall_k,
        embedder=embedder,
        fast=args.fast,
        granularity=args.granularity,
        query_expand=args.query_expand,
        dual_key=args.dual_key,
        pref_facts_key=args.pref_facts_key,
        brain_sleep=args.brain_sleep,
    )
    session_hit = session_in_recalls(
        recalls, item.get("answer_session_ids") or [], top_k=args.top_k
    )
    sids = session_ids_from_recalls(recalls, top_k=args.reader_top_k)
    if args.reader_con and brain is not None:
        from brain_memory import READER_TEMPLATE_CON, format_brain_reader_context  # noqa: WPS433

        context = format_brain_reader_context(
            item, recalls, brain, session_ids=sids, max_notes=56
        )
        reader_prompt = READER_TEMPLATE_CON.format(
            context=context,
            question_date=item.get("question_date") or "",
            question=item.get("question") or "",
        )
    else:
        history = format_history_json(item, sids)
        tmpl = READER_TEMPLATE_COT if args.reader_cot else READER_TEMPLATE
        reader_prompt = tmpl.format(
            history=history,
            question_date=item.get("question_date") or "",
            question=item.get("question") or "",
        )
    hypothesis = ""
    label: Optional[bool] = None
    if not args.skip_llm:
        hypothesis = llm_chat(
            reader_prompt,
            backend=args.llm_backend,
            model=reader_model,
            timeout_s=args.llm_timeout,
            max_tokens=args.reader_max_tokens,
        )
        label = judge_label(
            item,
            hypothesis,
            backend=args.llm_backend,
            judge_model=judge_model,
            timeout_s=args.llm_timeout,
            max_tokens=args.judge_max_tokens,
        )
    return {
        "question_id": item.get("question_id"),
        "question_type": item.get("question_type"),
        "session_recall_hit": session_hit,
        "retrieved_sessions": sids,
        "reader_top_k": args.reader_top_k,
        "reader_cot": args.reader_cot,
        "reader_con": args.reader_con,
        "bench_mode": args.bench_mode,
        "brain_sleep": args.brain_sleep,
        "ingested": ingested,
        "hypothesis": hypothesis,
        "autoeval_label": label,
        "reader_model": reader_model if not args.skip_llm else None,
        "judge_model": judge_model if not args.skip_llm else None,
        "llm_backend": args.llm_backend if not args.skip_llm else None,
        "sec": round(time.perf_counter() - t_q, 3),
    }


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
    ap.add_argument("--top-k", type=int, default=8, help="session recall@k metric (paper: 8)")
    ap.add_argument(
        "--reader-top-k",
        type=int,
        default=0,
        help="sessions fed to reader (0 = use profile/default; max profile uses 50)",
    )
    ap.add_argument(
        "--reader-cot",
        action="store_true",
        help="official LongMemEval chain-of-thought reader prompt",
    )
    ap.add_argument(
        "--reader-con",
        action="store_true",
        help="Chain-of-Note brain reader (cortex facts + hippocampal notes)",
    )
    ap.add_argument(
        "--bench-mode",
        default="",
        help="retrieve mode: brain | index | conv (0 = use e2e-profile default)",
    )
    ap.add_argument(
        "--brain-sleep",
        type=int,
        default=-1,
        help="CLS sleep cycles after haystack ingest (brain mode; -1 = profile default)",
    )
    ap.add_argument(
        "--reader-max-tokens",
        type=int,
        default=0,
        help="reader completion budget (0 = profile default)",
    )
    ap.add_argument(
        "--judge-max-tokens",
        type=int,
        default=0,
        help="judge completion budget (0 = profile default)",
    )
    ap.add_argument(
        "--e2e-profile",
        default=os.environ.get("LONGMEMEVAL_E2E_PROFILE", "brain"),
        choices=tuple(E2E_PROFILES),
        help="standard=gpt-4o; max=GPT-5+CoT+top-50; brain=CLS+CoN+completion (default for agents)",
    )
    ap.add_argument("--reader-model", default=None)
    ap.add_argument("--judge-model", default=None)
    ap.add_argument(
        "--llm-backend",
        default=os.environ.get("LONGMEMEVAL_LLM_BACKEND", DEFAULT_LLM_BACKEND),
        choices=("gemini", "openrouter", "cerebras", "groq", "openai", "cursor"),
        help="reader/judge API (OpenAI-compatible chat; cursor = slow Cloud Agents API)",
    )
    ap.add_argument(
        "--llm-timeout",
        type=int,
        default=int(os.environ.get("LONGMEMEVAL_LLM_TIMEOUT", "180")),
        help="seconds per reader/judge chat completion",
    )
    ap.add_argument(
        "--cursor-timeout",
        type=int,
        default=int(os.environ.get("CURSOR_API_TIMEOUT", "300")),
        help="alias kept for scripts; used when --llm-backend cursor",
    )
    ap.add_argument("--granularity", default="session", choices=("session", "turn"))
    ap.add_argument("--dual-key", action="store_true")
    ap.add_argument("--pref-facts-key", action="store_true")
    ap.add_argument("--query-expand", action="store_true")
    ap.add_argument("--fast", action="store_true", help="lexical retrieval only")
    ap.add_argument("--skip-llm", action="store_true", help="retrieval + prompt only")
    ap.add_argument("--skip-smoke-test", action="store_true", help="skip LLM ping at startup")
    ap.add_argument("--json-out", type=Path, default=None)
    ap.add_argument("--checkpoint", type=Path, default=None)
    ap.add_argument(
        "--workers",
        type=int,
        default=int(os.environ.get("LONGMEMEVAL_E2E_WORKERS", "1")),
        help="parallel questions (each worker uses its own embed cache)",
    )
    ap.add_argument(
        "--type-filter",
        default="",
        help="comma-separated question_type filter",
    )
    args = ap.parse_args()
    if args.llm_backend == "cursor" and args.llm_timeout == 120 and args.cursor_timeout != 300:
        args.llm_timeout = args.cursor_timeout

    prof = E2E_PROFILES.get(args.e2e_profile, E2E_PROFILES["standard"])
    if not args.reader_top_k:
        args.reader_top_k = int(prof["reader_top_k"])
    if not args.reader_max_tokens:
        args.reader_max_tokens = int(prof["reader_max_tokens"])
    if not args.judge_max_tokens:
        args.judge_max_tokens = int(prof["judge_max_tokens"])
    if args.e2e_profile == "max":
        args.reader_cot = True
    if args.e2e_profile == "brain":
        args.reader_con = True
        args.dual_key = True
        args.pref_facts_key = True
        args.query_expand = True
    if not args.bench_mode:
        args.bench_mode = str(prof.get("bench_mode") or "index")
    if args.brain_sleep < 0:
        args.brain_sleep = int(prof.get("brain_sleep") or 0)
    if args.e2e_profile in ("max", "brain") and args.llm_timeout == 180:
        args.llm_timeout = max(args.llm_timeout, 300)

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
    backend = args.llm_backend
    from cloud_llm import PROVIDERS

    default_model = (
        "gpt-4o-2024-08-06"
        if backend == "openai"
        else "auto"
        if backend == "cursor"
        else PROVIDERS.get(backend, {}).get("default_model")
    )
    reader_model = args.reader_model or default_model or READER_MODEL
    if args.judge_model:
        judge_model = args.judge_model
    elif backend == "openai":
        judge_model = str(prof.get("judge_model_openai", "gpt-4o-2024-08-06"))
    else:
        judge_model = reader_model
    if backend == "openai" and args.e2e_profile == "max" and not args.reader_model:
        reader_model = str(prof.get("reader_model_openai", reader_model))

    if not args.skip_llm:
        load_env_file()
        if backend == "cursor":
            if not load_cursor_api_key():
                raise SystemExit("Set CURSOR_API_KEY or pass --skip-llm.")
        else:
            if not args.skip_smoke_test:
                try:
                    smoke_test(backend, model=reader_model if backend == "openai" else None)
                except Exception as e:
                    raise SystemExit(
                        f"LLM backend {backend!r} failed smoke test: {e}\n"
                        "Set Colab Secret (GEMINI_API_KEY or OPENAI_API_KEY) or pass --skip-smoke-test."
                    ) from e

    pending: list[tuple[int, dict]] = []
    for i, item in enumerate(items):
        qid = str(item.get("question_id") or i)
        if qid not in done:
            pending.append((i, item))

    ckpt_lock = threading.Lock()
    rows_lock = threading.Lock()

    def run_item(_i: int, item: dict) -> dict:
        local_embedder = embedder if args.workers <= 1 else EmbedCache()
        last_err: Exception | None = None
        for attempt in range(4):
            try:
                return process_one_item(
                    item,
                    args=args,
                    embedder=local_embedder,
                    reader_model=reader_model,
                    judge_model=judge_model,
                )
            except Exception as e:
                last_err = e
                if attempt < 3 and ("429" in str(e) or "503" in str(e)):
                    time.sleep(min(60, 5 * (attempt + 1)))
                    continue
                raise
        assert last_err is not None
        raise last_err

    def record_row(row: dict) -> None:
        with rows_lock:
            rows.append(row)
            n = len(rows)
            if n % 5 == 0 or n == len(items):
                agg = aggregate_qa(rows)
                print(
                    f"[{n}/{len(items)}] session@k={agg['session_recall_at_k']:.1%} "
                    f"e2e={agg['overall_accuracy']:.1%} last_sec={row['sec']}",
                    flush=True,
                )
        if args.checkpoint:
            args.checkpoint.parent.mkdir(parents=True, exist_ok=True)
            with ckpt_lock:
                with args.checkpoint.open("a") as f:
                    f.write(json.dumps(row) + "\n")

    t0 = time.perf_counter()
    if args.workers <= 1:
        for i, item in pending:
            record_row(run_item(i, item))
    else:
        with ThreadPoolExecutor(max_workers=args.workers) as pool:
            futures = {pool.submit(run_item, i, item): item for i, item in pending}
            for fut in as_completed(futures):
                record_row(fut.result())

    order = {str(it.get("question_id") or idx): idx for idx, it in enumerate(items)}
    rows.sort(key=lambda r: order.get(str(r.get("question_id")), 10**9))

    wall = time.perf_counter() - t0
    summary = {
        "benchmark": "longmemeval_s_e2e",
        "harness": "v4",
        "dataset": str(args.data),
        "granularity": args.granularity,
        "top_k": args.top_k,
        "reader_top_k": args.reader_top_k,
        "reader_cot": args.reader_cot,
        "reader_con": args.reader_con,
        "bench_mode": args.bench_mode,
        "brain_sleep": args.brain_sleep,
        "reader_max_tokens": args.reader_max_tokens,
        "e2e_profile": args.e2e_profile,
        "dual_key": args.dual_key,
        "pref_facts_key": args.pref_facts_key,
        "query_expand": args.query_expand,
        "reader_model": reader_model,
        "judge_model": judge_model,
        "llm_backend": backend,
        "skip_llm": args.skip_llm,
        "questions": len(rows),
        "wall_s": round(wall, 1),
        "sec_per_question": round(wall / len(rows), 2) if rows else 0.0,
        "workers": max(1, args.workers),
        **aggregate_qa(rows),
    }
    out = args.json_out or REPO / "benchmarks/results/longmemeval-e2e-v4-mpnet.json"
    payload = {"summary": summary, "results": rows}
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(payload, indent=2))
    print(json.dumps(summary, indent=2))
    print(f"Wrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
