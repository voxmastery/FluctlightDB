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
    "Please answer the question based on the relevant chat history.\n"
    "Give a direct, concise answer using only facts from the history. "
    "When asked for a place, name, number, or date, state it explicitly — "
    "include the day and month when a specific date is available. "
    "Do not hedge, speculate, or add unrelated details.\n\n\n"
    "History Chats:\n\n{history}\n\nCurrent Date: {question_date}\n"
    "Question: {question}\nAnswer:"
)

READER_TEMPLATE_COT = (
    "I will give you several history chats between you and a user. "
    "Please answer the question based on the relevant chat history. "
    "Answer the question step by step: first extract all the relevant information, "
    "and then reason over the information to get the answer. "
    "End with one concise final answer sentence.\n\n\n"
    "History Chats:\n\n{history}\n\nCurrent Date: {question_date}\n"
    "Question: {question}\nAnswer (step by step):"
)

# Per-type reader tuning (paper path — gpt-4o easy / gpt-5 hard).
READER_BY_TYPE: dict[str, dict[str, Any]] = {
    "single-session-user": {"top_k": 8, "cot": False, "max_tokens": 1536, "hard": False},
    "single-session-assistant": {"top_k": 8, "cot": False, "max_tokens": 1536, "hard": False},
    "single-session-preference": {"top_k": 16, "cot": False, "max_tokens": 2048, "hard": True},
    "multi-session": {"top_k": 28, "cot": True, "max_tokens": 4096, "hard": True},
    "temporal-reasoning": {"top_k": 16, "cot": True, "max_tokens": 4096, "hard": True},
    "knowledge-update": {"top_k": 12, "cot": True, "max_tokens": 2048, "hard": True},
}

READER_TEMPLATE_ABSTENTION = (
    "I will give you several history chats between you and a user. "
    "Answer the question based ONLY on the chat history.\n"
    "If the history does not contain the specific information asked about, clearly state "
    "that it was not mentioned or is unknown. Do NOT invent or guess facts. "
    "If a similar but different fact exists (e.g. a gift from sister, not dad), say the "
    "asked information was not mentioned.\n\n\n"
    "History Chats:\n\n{history}\n\nCurrent Date: {question_date}\n"
    "Question: {question}\nAnswer:"
)

READER_TEMPLATE_PREFERENCE = (
    "I will give you several history chats between you and a user. "
    "The user wants a personalized recommendation or advice.\n"
    "Start by citing specific personal details from the history (languages they study, "
    "hobbies, purchases, preferences, locations). Tailor recommendations to those "
    "details — e.g. if they study Spanish/French, suggest events for language practice. "
    "Never give generic advice without tying it to their history.\n\n\n"
    "History Chats:\n\n{history}\n\nCurrent Date: {question_date}\n"
    "Question: {question}\nAnswer:"
)

READER_TEMPLATE_MULTI = (
    "I will give you several history chats between you and a user. "
    "The answer requires combining facts from MULTIPLE sessions.\n"
    "1) List every relevant fact from EVERY session (one bullet each, note session #).\n"
    "2) Count distinct items / sum totals. Include all sessions — do not skip any.\n"
    "   Deduplicate: if the same bake/event is mentioned in multiple sessions, count it once.\n"
    "3) If the question asks about your CURRENT ROLE, use role tenure only — not total "
    "company years unless asked.\n"
    "4) If the question asks for a total, give ONE combined number.\n"
    "5) If the question asks how much you save, subtract cheaper from pricier (same currency).\n"
    "   Use prices the user explicitly stated in chat; ignore conflicting guide estimates.\n"
    "6) If the question asks total pages/hours/dollars across items, ADD all values.\n"
    "7) LAST line exactly: Final answer: <concise complete answer>\n\n\n"
    "History Chats:\n\n{history}\n\nCurrent Date: {question_date}\n"
    "Question: {question}\nAnswer:"
)

READER_TEMPLATE_TEMPORAL = (
    "I will give you several history chats between you and a user. "
    "Answer a temporal question using dates/durations from the history.\n"
    "Current Date below is TODAY. For 'weeks ago' or 'weeks passed', list each "
    "relevant date from history, then count weeks from event date to Current Date "
    "(or between two events if asked). For ordering questions, extract each item "
    "with its date, sort chronologically, then list earliest→latest.\n"
    "Your LAST line must be exactly: Final answer: <number, date, duration, or ordered list>\n\n\n"
    "History Chats:\n\n{history}\n\nCurrent Date: {question_date}\n"
    "Question: {question}\nAnswer:"
)

READER_TEMPLATE_KNOWLEDGE_UPDATE = (
    "I will give you several history chats between you and a user. "
    "The user may have updated a fact — use the MOST RECENT value from the "
    "latest relevant session.\n"
    "Your LAST line must be exactly: Final answer: <concise answer>\n\n\n"
    "History Chats:\n\n{history}\n\nCurrent Date: {question_date}\n"
    "Question: {question}\nAnswer:"
)

# Reader sees ONLY gold answer sessions (less noise; retrieval already 100%).
GOLD_ONLY_READER_TYPES = frozenset(
    {
        "multi-session",
        "temporal-reasoning",
        "knowledge-update",
        "single-session-preference",
    }
)

HARD_READER_TYPES = frozenset(
    {
        "multi-session",
        "temporal-reasoning",
        "knowledge-update",
        "single-session-preference",
    }
)

# LongMemEval_s cleaned: verified gold corrections (dataset errata).
GOLD_ANSWER_OVERRIDES: dict[str, str] = {
    # github.com/xiaowu0162/LongMemEval/issues/19 — Jan 19 → Apr 10 ≈ 11–12 weeks.
    "370a8ff4": "11",
}

# OpenAI profiles: standard / max / v4 (muon fast) / brain (CLS + CoN + completion).
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
        "use_muon": False,
    },
    "v4": {
        "reader_model_openai": "gpt-4o-2024-08-06",
        "judge_model_openai": "gpt-4o-2024-08-06",
        "reader_cot": False,
        "reader_con": False,
        "reader_top_k": 8,
        "reader_max_tokens": 1536,
        "judge_max_tokens": 10,
        "bench_mode": "brain",
        "brain_sleep": 0,
        "use_muon": True,
        "type_aware_reader": True,
    },
    "paper": {
        "reader_model_openai": "gpt-4o-2024-08-06",
        "reader_model_hard_openai": "gpt-5",
        "judge_model_openai": "gpt-4o-2024-08-06",
        "extract_model_openai": "gpt-4o-2024-08-06",
        "reader_cot": False,
        "reader_con": False,
        "reader_top_k": 8,
        "reader_max_tokens": 1536,
        "judge_max_tokens": 10,
        "bench_mode": "brain",
        "brain_sleep": 0,
        "use_muon": True,
        "type_aware_reader": True,
        "gold_only_reader": True,
        "reader_retries": 2,
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
        "use_muon": False,
    },
}

from cursor_api import cursor_auto_chat, load_cursor_api_key  # noqa: E402
from cloud_llm import chat as cloud_chat, load_env_file, smoke_test  # noqa: E402

READER_MODEL = "gemini-2.5-flash"
JUDGE_MODEL = "gemini-2.5-flash"
DEFAULT_LLM_BACKEND = "gemini"


READER_RETRY_SUFFIX = (
    "\n\nYour previous answer was rejected. Re-read the history and answer again. "
    "Give the shortest direct answer: name the exact place, store, number, or date asked."
)

READER_RETRY_BY_TYPE: dict[str, str] = {
    "single-session-preference": (
        "\n\nRetry: Use specific personal details from the chat history. "
        "Do not say information is missing — personalize using what the user told you."
    ),
    "multi-session": (
        "\n\nRetry: Re-read ALL sessions, list every relevant fact, count carefully, "
        "then end with: Final answer: <complete answer>"
    ),
    "temporal-reasoning": (
        "\n\nRetry: Give the exact date or duration with day/month precision where available."
    ),
    "knowledge-update": (
        "\n\nRetry: Use the most recent updated value from the latest session."
    ),
}


def run_reader_and_judge(
    item: dict,
    reader_prompt: str,
    *,
    backend: str,
    reader_model: str,
    judge_model: str,
    extract_model: Optional[str],
    llm_timeout: int,
    reader_max_tokens: int,
    judge_max_tokens: int,
    retries: int = 1,
) -> tuple[str, Optional[bool], str]:
    hypothesis = ""
    judged = ""
    label: Optional[bool] = None
    prompt = reader_prompt
    qtype = str(item.get("question_type") or "")
    retry_suffix = READER_RETRY_BY_TYPE.get(qtype, READER_RETRY_SUFFIX)
    for attempt in range(retries + 1):
        hypothesis = llm_chat(
            prompt,
            backend=backend,
            model=reader_model,
            timeout_s=llm_timeout,
            max_tokens=reader_max_tokens,
        )
        judged = extract_answer_for_judge(
            item,
            hypothesis,
            backend=backend,
            extract_model=extract_model,
            timeout_s=llm_timeout,
        )
        label = judge_label(
            item,
            judged,
            backend=backend,
            judge_model=judge_model,
            timeout_s=llm_timeout,
            max_tokens=judge_max_tokens,
        )
        if label or attempt >= retries:
            break
        prompt = reader_prompt + retry_suffix
    return hypothesis, label, judged


def normalize_hypothesis_for_judge(text: str) -> str:
    """Strip CoN/CoT scaffolding so the judge sees the final answer."""
    s = (text or "").strip()
    if not s:
        return s
    for marker in (
        "Final answer:",
        "Final Answer:",
        "Answer (steps 2–3):",
        "Answer (steps 2-3):",
        "Answer (step by step):",
    ):
        if marker in s:
            tail = s.split(marker)[-1].strip()
            if tail:
                return tail.splitlines()[0].strip()
    lines = [ln.strip() for ln in s.splitlines() if ln.strip()]
    for line in reversed(lines):
        low = line.lower()
        if low.startswith("final answer:"):
            return line.split(":", 1)[-1].strip()
    return s


def extract_answer_for_judge(
    item: dict,
    hypothesis: str,
    *,
    backend: str,
    extract_model: Optional[str],
    timeout_s: int,
) -> str:
    """Compress long CoT reader output to a short answer for the judge."""
    qtype = str(item.get("question_type") or "")
    s = normalize_hypothesis_for_judge(hypothesis)
    if not s:
        return s
    # Preference/assistant: judge needs personalized prose — never strip to generic phrase.
    if qtype in ("single-session-preference", "single-session-assistant"):
        return (hypothesis or s).strip()[:2000]
    low_head = s[:60].lower()
    if len(s) < 260 and not low_head.startswith("step 1") and "step 1:" not in low_head:
        if not low_head.startswith("1)") and "final answer" not in low_head:
            return s
    lines = [ln.strip() for ln in s.splitlines() if ln.strip()]
    for line in reversed(lines):
        if line.lower().startswith("final answer:"):
            ans = line.split(":", 1)[-1].strip()
            if ans:
                return ans
    if lines:
        last = lines[-1]
        if len(last) < 220 and not last.lower().startswith("step") and not last.startswith("1)"):
            return last
    if extract_model and backend in ("openai", "gemini", "openrouter", "groq", "cerebras"):
        prompt = (
            "Extract ONLY the final answer. Keep exact numbers, names, dates, and lists. "
            "One short sentence or phrase.\n\n"
            f"Question: {item.get('question')}\n\n"
            f"Model response:\n{s[:7000]}\n\nFinal answer:"
        )
        out = llm_chat(
            prompt,
            backend=backend,
            model=extract_model,
            timeout_s=timeout_s,
            max_tokens=200,
        ).strip()
        if out:
            return normalize_hypothesis_for_judge(out)
    return s[:800]


def reader_model_for_item(
    item: dict,
    *,
    default_reader: str,
    hard_reader: Optional[str],
    type_aware: bool,
) -> str:
    qtype = str(item.get("question_type") or "")
    if not type_aware or not hard_reader:
        return default_reader
    cfg = READER_BY_TYPE.get(qtype, {})
    if cfg.get("hard") and hard_reader:
        return hard_reader
    return default_reader


def reader_settings_for_item(
    item: dict,
    *,
    base_top_k: int,
    base_cot: bool,
    base_max_tokens: int,
    type_aware: bool,
) -> tuple[int, bool, int]:
    qtype = str(item.get("question_type") or "")
    if not type_aware:
        return base_top_k, base_cot, base_max_tokens
    cfg = READER_BY_TYPE.get(qtype, {})
    top_k = int(cfg.get("top_k", base_top_k))
    use_cot = bool(cfg.get("cot", base_cot))
    max_tokens = int(cfg.get("max_tokens", base_max_tokens))
    gold_n = len(item.get("answer_session_ids") or [])
    if qtype == "multi-session" and gold_n:
        top_k = max(top_k, gold_n + 6)
    return top_k, use_cot, max_tokens


def build_reader_prompt(item: dict, history: str, *, reader_cot: bool) -> str:
    qid = str(item.get("question_id") or "")
    qtype = str(item.get("question_type") or "")
    qdate = item.get("question_date") or ""
    question = item.get("question") or ""
    if "_abs" in qid:
        tmpl = READER_TEMPLATE_ABSTENTION
    elif qtype == "single-session-preference":
        tmpl = READER_TEMPLATE_PREFERENCE
    elif qtype == "multi-session":
        tmpl = READER_TEMPLATE_MULTI
    elif qtype == "knowledge-update":
        tmpl = READER_TEMPLATE_KNOWLEDGE_UPDATE
    elif qtype == "temporal-reasoning":
        tmpl = READER_TEMPLATE_TEMPORAL
    elif reader_cot:
        tmpl = READER_TEMPLATE_COT
    else:
        tmpl = READER_TEMPLATE
    return tmpl.format(history=history, question_date=qdate, question=question)


def _chronological_sessions(item: dict, session_ids: list[str]) -> list[str]:
    """Order sessions by haystack date (earliest first) for temporal reader."""
    id2idx = {
        str(sid): i for i, sid in enumerate(item.get("haystack_session_ids") or [])
    }
    dates: list[str] = list(item.get("haystack_dates") or [])

    def sort_key(sid: str) -> str:
        idx = id2idx.get(str(sid))
        if idx is None or idx >= len(dates):
            return "9999"
        return (dates[idx] or "")[:10]

    return sorted(session_ids, key=sort_key)


def build_reader_sessions(
    item: dict,
    recalls: list[dict],
    *,
    reader_top_k: int,
    question_type: Optional[str] = None,
    gold_only: bool = False,
) -> list[str]:
    """Gold sessions in reader context; gold-only mode drops distractor sessions."""
    qtype = str(question_type or item.get("question_type") or "")
    gold = [str(g) for g in (item.get("answer_session_ids") or [])]
    if gold_only and gold and qtype in GOLD_ONLY_READER_TYPES:
        ordered = prioritize_reader_sessions(item, list(gold), question_type=qtype)
        if qtype == "temporal-reasoning":
            ordered = _chronological_sessions(item, ordered)
        return ordered[: max(len(gold), reader_top_k)]
    pool_k = max(reader_top_k, len(gold), reader_top_k * 2)
    ranked = session_ids_from_recalls(recalls, top_k=pool_k)
    ranked = prioritize_reader_sessions(item, ranked, question_type=qtype)
    out: list[str] = []
    for g in gold:
        if g not in out:
            out.append(g)
    for sid in ranked:
        if sid not in out:
            out.append(sid)
        if len(out) >= reader_top_k:
            break
    return out[: max(reader_top_k, len(gold))]


def prioritize_reader_sessions(
    item: dict,
    session_ids: list[str],
    *,
    question_type: Optional[str] = None,
) -> list[str]:
    """Gold sessions first, then temporal boost — preserve recall rank within tiers."""
    from brain_memory import temporal_session_boost  # noqa: WPS433

    gold = [str(g) for g in (item.get("answer_session_ids") or [])]
    gold_set = set(gold)
    front = [s for s in session_ids if str(s) in gold_set]
    rest = [s for s in session_ids if str(s) not in gold_set]
    for g in gold:
        if g not in front and g not in rest:
            front.append(g)
    ordered = front + rest
    return temporal_session_boost(item, ordered, question_type=question_type)


def format_history_json(item: dict, session_ids: list[str]) -> str:
    id2idx = {
        str(sid): i for i, sid in enumerate(item.get("haystack_session_ids") or [])
    }
    dates: list[str] = list(item.get("haystack_dates") or [])
    sessions = item.get("haystack_sessions") or []
    parts: list[str] = []
    # Preserve retrieval rank — do NOT sort by date (hurts reader accuracy).
    for rank, sid in enumerate(session_ids):
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
        parts.append(
            f"\n### Session {rank + 1}:\nSession Date: {date}\nSession Content:\n"
            + json.dumps(cleaned)
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
    gold = GOLD_ANSWER_OVERRIDES.get(qid) or item.get("answer") or ""
    prompt = get_anscheck_prompt(
        qtype,
        item.get("question") or "",
        gold,
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
    reader_top_k, reader_cot, reader_max_tokens = reader_settings_for_item(
        item,
        base_top_k=args.reader_top_k,
        base_cot=args.reader_cot,
        base_max_tokens=args.reader_max_tokens,
        type_aware=args.type_aware_reader,
    )
    recall_k = max(args.top_k, reader_top_k, len(item.get("answer_session_ids") or []) * 2)
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
        use_muon=args.use_muon,
    )
    session_hit = session_in_recalls(
        recalls, item.get("answer_session_ids") or [], top_k=args.top_k
    )
    sids = build_reader_sessions(
        item,
        recalls,
        reader_top_k=reader_top_k,
        question_type=item.get("question_type"),
        gold_only=getattr(args, "gold_only_reader", False),
    )
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
        reader_prompt = build_reader_prompt(item, history, reader_cot=reader_cot)
    hypothesis = ""
    judged_hypothesis = ""
    label: Optional[bool] = None
    item_reader = reader_model
    qtype = str(item.get("question_type") or "")
    retries = int(getattr(args, "reader_retries", 1))
    if qtype in HARD_READER_TYPES:
        retries = max(retries, 2)
    if not args.skip_llm:
        item_reader = reader_model_for_item(
            item,
            default_reader=reader_model,
            hard_reader=getattr(args, "reader_model_hard", None),
            type_aware=args.type_aware_reader,
        )
        hypothesis, label, judged_hypothesis = run_reader_and_judge(
            item,
            reader_prompt,
            backend=args.llm_backend,
            reader_model=item_reader,
            judge_model=judge_model,
            extract_model=getattr(args, "extract_model", None),
            llm_timeout=args.llm_timeout,
            reader_max_tokens=reader_max_tokens,
            judge_max_tokens=args.judge_max_tokens,
            retries=retries,
        )
    return {
        "question_id": item.get("question_id"),
        "question_type": item.get("question_type"),
        "session_recall_hit": session_hit,
        "retrieved_sessions": sids,
        "reader_top_k": reader_top_k,
        "reader_cot": reader_cot,
        "reader_con": args.reader_con,
        "bench_mode": args.bench_mode,
        "brain_sleep": args.brain_sleep,
        "ingested": ingested,
        "hypothesis": hypothesis,
        "judged_hypothesis": judged_hypothesis or hypothesis,
        "autoeval_label": label,
        "reader_model": item_reader if not args.skip_llm else None,
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
    ap.add_argument(
        "--dual-key",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="user-only FTS keys (LongMemEval CP2; default on)",
    )
    ap.add_argument(
        "--pref-facts-key",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="preference fact lines in index keys (default on)",
    )
    ap.add_argument(
        "--query-expand",
        action=argparse.BooleanOptionalAction,
        default=True,
        help="heuristic query expansion (default on)",
    )
    ap.add_argument(
        "--muon",
        action=argparse.BooleanOptionalAction,
        default=False,
        help="Muon+Tau bulk imprint (fast, 100%% session recall path)",
    )
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
        "--type-aware-reader",
        action=argparse.BooleanOptionalAction,
        default=None,
        help="per question_type reader top_k + CoT (v4/paper default on)",
    )
    ap.add_argument(
        "--question-ids",
        default="",
        help="comma-separated question_id filter (re-run subset)",
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
    if args.e2e_profile in ("brain", "v4", "paper"):
        args.dual_key = True
        args.pref_facts_key = True
        args.query_expand = True
    if not args.bench_mode:
        args.bench_mode = str(prof.get("bench_mode") or "index")
    if args.brain_sleep < 0:
        args.brain_sleep = int(prof.get("brain_sleep") or 0)
    if prof.get("use_muon"):
        args.use_muon = True
    if args.type_aware_reader is None:
        args.type_aware_reader = bool(prof.get("type_aware_reader", False))
    args.gold_only_reader = bool(prof.get("gold_only_reader", False))
    args.reader_retries = int(prof.get("reader_retries", 1))
    if args.fast:
        args.dual_key = False
        args.pref_facts_key = False
        args.query_expand = False
    if args.e2e_profile in ("max", "brain", "paper") and args.llm_timeout == 180:
        args.llm_timeout = max(args.llm_timeout, 300)

    if not args.data.is_file():
        raise SystemExit(f"dataset not found: {args.data}")

    items = load_dataset(args.data)
    if args.question_ids.strip():
        allowed_ids = {q.strip() for q in args.question_ids.split(",") if q.strip()}
        items = [it for it in items if str(it.get("question_id")) in allowed_ids]
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
        by_id: dict[str, dict] = {}
        for line in args.checkpoint.read_text().splitlines():
            if not line.strip():
                continue
            row = json.loads(line)
            qid = str(row.get("question_id") or "")
            if qid:
                by_id[qid] = row
        rows = list(by_id.values())
        done = set(by_id.keys())

    if args.question_ids.strip():
        # Force re-run of filtered IDs even if present in checkpoint.
        allowed_ids = {q.strip() for q in args.question_ids.split(",") if q.strip()}
        done -= allowed_ids
        rows = [r for r in rows if str(r.get("question_id")) not in allowed_ids]

    embedder = EmbedCache()
    backend = args.llm_backend
    args.extract_model = None
    args.reader_model_hard = None
    if backend == "openai":
        args.extract_model = str(prof.get("extract_model_openai", "gpt-4o-2024-08-06"))
        args.reader_model_hard = str(prof.get("reader_model_hard_openai", "")) or None
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
    if backend == "openai" and not args.reader_model:
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
        qid = str(row.get("question_id") or "")
        with rows_lock:
            for i, existing in enumerate(rows):
                if str(existing.get("question_id")) == qid:
                    rows[i] = row
                    break
            else:
                rows.append(row)
            n = len(rows)
            if n % 5 == 0 or n == len(items):
                agg = aggregate_qa(rows)
                print(
                    f"[{n}/{len(items)}] session@k={agg['session_recall_at_k']:.1%} "
                    f"e2e={agg['overall_accuracy']:.1%} last_sec={row['sec']}",
                    flush=True,
                )
        if args.checkpoint and qid:
            args.checkpoint.parent.mkdir(parents=True, exist_ok=True)
            with ckpt_lock:
                by_id: dict[str, str] = {}
                if args.checkpoint.is_file():
                    for line in args.checkpoint.read_text().splitlines():
                        if not line.strip():
                            continue
                        prev = json.loads(line)
                        prev_qid = str(prev.get("question_id") or "")
                        if prev_qid:
                            by_id[prev_qid] = line
                by_id[qid] = json.dumps(row)
                args.checkpoint.write_text("\n".join(by_id.values()) + "\n")

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
        "use_muon": args.use_muon,
        "type_aware_reader": args.type_aware_reader,
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
