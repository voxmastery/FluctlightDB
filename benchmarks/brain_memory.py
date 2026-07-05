#!/usr/bin/env python3
"""Brain-native LongMemEval memory — CLS consolidation, fact engrams, completion recall.

Maps neuroscience → FluctlightDB:
  - Dentate separation + turn/fact granularity (pattern separation)
  - Sleep replay → cortex consolidation (McClelland CLS)
  - Parallel pathways: lexical + semantic + cortical fact boost
  - CA3 pattern completion via ``complete(cue)``
  - Temporal indexing (entorhinal time context)
  - Chain-of-Note reader context (PFC-style structured extraction)
"""

from __future__ import annotations

import re
from typing import Any, Optional

FIRST_PERSON = re.compile(
    r"\b(i|i'm|i've|i'd|i'll|my|mine|we|we're|we've|our)\b", re.I
)
DATE_IN_QUESTION = re.compile(
    r"\b(20\d{2}[-/]\d{1,2}[-/]\d{1,2}|\d{1,2}/\d{1,2}/20\d{2}|"
    r"january|february|march|april|may|june|july|august|september|"
    r"october|november|december)\b",
    re.I,
)


def extract_atomic_facts(
    role: str,
    content: str,
    *,
    date: str = "",
    session_id: str = "",
) -> list[str]:
    """User-turn atomic facts for index–value engrams (hippocampal keys → cortical values)."""
    if not content or role != "user":
        return []
    prefix = f"[{date}] " if date else ""
    facts: list[str] = []
    for sent in re.split(r"(?<=[.!?])\s+", content.strip()):
        s = sent.strip()
        if len(s) < 12:
            continue
        if FIRST_PERSON.search(s) or any(
            c in s.lower()
            for c in (
                "bought",
                "purchased",
                "graduated",
                "commute",
                "volunteer",
                "coupon",
                "mbps",
                "yoga",
                "degree",
                "redeemed",
                "upgraded",
            )
        ):
            facts.append(f"{prefix}{s[:380]}")
    # Dedupe
    seen: set[str] = set()
    out: list[str] = []
    for f in facts:
        k = f.lower()[:80]
        if k not in seen:
            seen.add(k)
            out.append(f)
    return out[:6]


def parse_question_date(question_date: str) -> Optional[str]:
    """Normalize haystack date string for filtering."""
    qd = (question_date or "").strip()
    if not qd:
        return None
    m = re.search(r"(20\d{2})[-/](\d{1,2})[-/](\d{1,2})", qd)
    if m:
        return f"{m.group(1)}-{int(m.group(2)):02d}-{int(m.group(3)):02d}"
    return qd[:10] if len(qd) >= 8 else None


def temporal_session_boost(
    item: dict,
    session_ids: list[str],
    *,
    question_type: Optional[str],
) -> list[str]:
    """Time-aware reorder: sessions near question_date first (LongMemEval CP temporal)."""
    if question_type != "temporal-reasoning":
        return session_ids
    qdate = parse_question_date(str(item.get("question_date") or ""))
    if not qdate:
        return session_ids
    id2idx = {
        str(sid): i for i, sid in enumerate(item.get("haystack_session_ids") or [])
    }
    dates: list[str] = list(item.get("haystack_dates") or [])

    def score(sid: str) -> tuple[int, int]:
        idx = id2idx.get(str(sid))
        if idx is None or idx >= len(dates):
            return (1, 9999)
        d = (dates[idx] or "")[:10]
        if qdate in d or d in qdate:
            return (0, 0)
        if d[:7] == qdate[:7]:
            return (0, 1)
        return (1, abs(hash(d)) % 10000)

    return sorted(session_ids, key=lambda s: score(str(s)))


def ingest_muon_haystack(
    brain: Any,
    item: dict,
    *,
    dual_key: bool = True,
    pref_facts_key: bool = True,
) -> int:
    """Muon Lane: one penetrative imprint per session (0 embed HTTP, 0 per-turn experience)."""
    from longmemeval_bench import preference_signals  # noqa: WPS433

    session_ids: list[str] = list(item.get("haystack_session_ids") or [])
    dates: list[str] = list(item.get("haystack_dates") or [])
    sessions = item.get("haystack_sessions") or []
    batch: list[dict[str, str]] = []

    for i, session in enumerate(sessions):
        if not isinstance(session, list):
            continue
        sid = session_ids[i] if i < len(session_ids) else f"session_{i}"
        date = dates[i] if i < len(dates) else ""
        lines: list[str] = []
        user_key: list[str] = []
        for msg in session:
            if not isinstance(msg, dict):
                continue
            role = (msg.get("role") or "user").strip()
            content = (msg.get("content") or "").strip()
            if not content:
                continue
            lines.append(f"{role}: {content}")
            if role == "user":
                user_key.append(content)
        if not lines:
            continue
        pref = preference_signals(user_key)
        key_block = " ".join(user_key)[:3000]
        body = "\n".join(lines)
        prefix = f"[{date}] " if date else ""
        user_keys = f"{prefix}{pref}\n{key_block}"[:4000]
        if dual_key and user_key:
            user_only = f"{prefix}{pref}\n" + "\n".join(
                f"user: {u}" for u in user_key
            )[:8000]
            user_keys = user_only[:4000]
        full = f"{prefix}{pref}\n{key_block}\n{body}"[:12000]
        batch.append(
            {
                "session_id": str(sid),
                "date": str(date),
                "body": full,
                "user_keys": user_keys,
            }
        )

    if not batch:
        return 0

    if hasattr(brain, "muon_imprint_batch"):
        if callable(getattr(brain, "muon_imprint_batch", None)):
            brain.muon_imprint_batch(batch)
            return len(batch)
    if hasattr(brain, "_post"):
        brain.muon_imprint_batch(batch)
        return len(batch)
    return 0


def tau_hits_to_recalls(hits: list[dict]) -> list[dict]:
    """Shape Tau episodic hits for LongMemEval session_in_recalls()."""
    recalls: list[dict] = []
    for h in hits:
        sid = h.get("session_id") or ""
        chunk = h.get("chunk_id") or "session"
        recalls.append(
            {
                "engram_id": h.get("shard_id") or f"tau:{sid}",
                "activation": float(h.get("score") or 0.0),
                "episode": {
                    "content": h.get("content") or h.get("snippet") or "",
                    "context": h.get("date") or "",
                    "rag": {"doc_id": sid, "chunk_id": chunk},
                    "doc_id": sid,
                },
                "tau": True,
            }
        )
    return recalls


def muon_hits_to_recalls(hits: list[dict]) -> list[dict]:
    """Shape Muon hits for LongMemEval session_in_recalls()."""
    recalls: list[dict] = []
    for h in hits:
        sid = h.get("session_id") or ""
        recalls.append(
            {
                "engram_id": f"muon:{sid}",
                "activation": float(h.get("score") or 0.0),
                "episode": {
                    "content": h.get("snippet") or "",
                    "context": h.get("date") or "",
                    "rag": {"doc_id": sid, "chunk_id": "session"},
                    "doc_id": sid,
                },
                "muon": True,
            }
        )
    return recalls


def muon_activate(
    brain: Any,
    question: str,
    *,
    question_type: Optional[str] = None,
    top_k: int = 8,
    query_expand: bool = True,
) -> list[dict]:
    """Penetrative episodic recall: Tau fission when available, else Muon sessions."""
    from longmemeval_bench import expand_queries, merge_recalls  # noqa: WPS433

    pool_k = max(top_k * 2, 16)
    if question_type == "single-session-preference":
        pool_k = max(top_k * 3, 24)

    def recall_one(q: str) -> list[dict]:
        if hasattr(brain, "tau_recall"):
            try:
                raw = brain.tau_recall(q, limit=pool_k)
                if raw:
                    return tau_hits_to_recalls(raw)
            except Exception:
                pass
        if hasattr(brain, "muon_recall"):
            raw = brain.muon_recall(q, limit=pool_k)
            if isinstance(raw, dict):
                raw = raw.get("hits") or []
            return muon_hits_to_recalls(raw if isinstance(raw, list) else [])
        return []

    queries = expand_queries(question, question_type) if query_expand else [question]
    if len(queries) == 1:
        return recall_one(queries[0])[: max(top_k * 2, 16)]
    lists = [recall_one(q) for q in queries]
    return merge_recalls(lists, top_k)


def ingest_brain_haystack(
    brain: Any,
    item: dict,
    embedder: Any,
    *,
    fast: bool,
    dual_key: bool,
    pref_facts_key: bool,
    turn_engrams: bool = True,
    fact_engrams: bool = True,
    sleep_cycles: int = 2,
) -> int:
    """Full brain ingest: session + turn + atomic facts + optional CLS sleep."""
    from longmemeval_bench import _ingest_sessions  # noqa: WPS433

    n = _ingest_sessions(
        brain,
        item,
        embedder,
        fast=fast,
        dual_key=dual_key,
        pref_facts_key=pref_facts_key,
    )
    session_ids: list[str] = list(item.get("haystack_session_ids") or [])
    dates: list[str] = list(item.get("haystack_dates") or [])
    sessions = item.get("haystack_sessions") or []

    embed_texts: list[str] = []
    payloads: list[tuple[str, str, str, str, Optional[list[float]]]] = []

    for i, session in enumerate(sessions):
        if not isinstance(session, list):
            continue
        sid = session_ids[i] if i < len(session_ids) else f"session_{i}"
        date = dates[i] if i < len(dates) else ""
        turn_n = 0
        for msg in session:
            if not isinstance(msg, dict):
                continue
            role = (msg.get("role") or "user").strip()
            content = (msg.get("content") or "").strip()
            if not content:
                continue
            if turn_engrams:
                if role != "user":
                    continue
                line = f"[{date}] {role}: {content[:520]}"
                embed_snip = line[:1200]
                embed_texts.append(embed_snip)
                payloads.append((sid, line, f"turn-{turn_n}", embed_snip, None))
                turn_n += 1
                if turn_n >= 12:
                    break
            if fact_engrams and role == "user":
                for fi, fact in enumerate(
                    extract_atomic_facts(role, content, date=date, session_id=sid)
                ):
                    body = f"[{date}] {sid} fact: {fact}"
                    embed_snip = body[:800]
                    embed_texts.append(embed_snip)
                    payloads.append((sid, body, f"fact-{fi}", embed_snip, None))

    vectors: dict[str, list[float]] = {}
    if not fast and embed_texts:
        unique = list(dict.fromkeys(embed_texts))
        got = embedder.embed_many(unique)
        for text, vec in zip(unique, got):
            if vec is not None:
                vectors[text] = vec

    for sid, content, chunk_id, embed_snip, _ in payloads:
        vec = vectors.get(embed_snip)
        salience = 0.78 if chunk_id.startswith("fact-") else 0.62
        brain.experience(
            content,
            context=f"longmemeval:{sid}",
            salience=salience,
            semantic_vector=vec,
            doc_id=sid,
            chunk_id=chunk_id,
        )
        n += 1

    if sleep_cycles > 0 and hasattr(brain, "sleep"):
        for _ in range(sleep_cycles):
            try:
                brain.sleep()
            except Exception:
                break
    return n


def brain_activate(
    brain: Any,
    item: dict,
    embedder: Any,
    *,
    question: str,
    question_type: Optional[str],
    fast: bool,
    top_k: int,
    query_expand: bool,
) -> list[dict]:
    """Parallel pathway recall: hybrid activate + pattern completion + cortical boost."""
    from longmemeval_bench import activate_merged  # noqa: WPS433

    recalls = activate_merged(
        brain,
        question,
        question_type=question_type,
        embedder=embedder,
        fast=fast,
        top_k=max(top_k, 16),
        query_expand=query_expand,
    )
    # CA3 pattern completion — prepend if strong match not already in pool
    if hasattr(brain, "complete"):
        try:
            completed = brain.complete(question)
        except Exception:
            completed = None
        if completed and isinstance(completed, dict):
            cid = completed.get("engram_id")
            if cid and not any(
                (r.get("engram_id") or r.get("episode", {}).get("id")) == cid for r in recalls
            ):
                recalls.insert(
                    0,
                    {
                        "engram_id": cid,
                        "activation": 1.25,
                        "episode": {
                            "content": completed.get("content") or "",
                            "context": completed.get("context") or "",
                        },
                        "completion": True,
                    },
                )
    return recalls[: max(top_k * 3, 24)]


def cortex_fact_lines(brain: Any, cue: str, limit: int = 20) -> list[str]:
    if not hasattr(brain, "cortex_facts"):
        return []
    try:
        raw = brain.cortex_facts(cue, limit=limit)
    except Exception:
        return []
    lines: list[str] = []
    if isinstance(raw, list):
        for row in raw:
            if isinstance(row, (list, tuple)) and row:
                lines.append(str(row[0]))
            elif isinstance(row, str):
                lines.append(row)
    return lines[:limit]


def recalls_to_notes(recalls: list[dict], limit: int = 40) -> list[str]:
    """Chain-of-Note: one note line per recalled engram (PFC extraction analog)."""
    notes: list[str] = []
    seen: set[str] = set()
    for r in recalls[:limit]:
        ep = r.get("episode") or {}
        content = (ep.get("content") or "").strip()
        if not content:
            continue
        rag = ep.get("rag") or {}
        sid = rag.get("doc_id") or ep.get("doc_id") or ""
        chunk = (rag.get("chunk_id") or "session").strip()
        key = f"{sid}:{chunk}:{content[:60]}"
        if key in seen:
            continue
        seen.add(key)
        snippet = content.replace("\n", " ")[:420]
        tag = "completion" if r.get("completion") else chunk
        notes.append(f"- [{tag}] {snippet}")
    return notes


def format_brain_reader_context(
    item: dict,
    recalls: list[dict],
    brain: Any,
    *,
    session_ids: list[str],
    max_notes: int = 48,
) -> str:
    """Minimal brain-native context: cortical facts + CoN notes + selected sessions."""
    from longmemeval_e2e import format_history_json  # noqa: WPS433

    parts: list[str] = []
    cue = (item.get("question") or "").strip()
    facts = cortex_fact_lines(brain, cue, limit=16)
    if facts:
        parts.append("Consolidated memory (cortex):\n" + "\n".join(f"- {f}" for f in facts))
    notes = recalls_to_notes(recalls, limit=max_notes)
    if notes:
        parts.append("Retrieved notes (hippocampal index):\n" + "\n".join(notes))
    ordered_sids = temporal_session_boost(
        item, session_ids, question_type=item.get("question_type")
    )
    history = format_history_json(item, ordered_sids[: min(12, len(ordered_sids))])
    if history.strip():
        parts.append("Session transcripts (selected):\n" + history)
    return "\n\n".join(parts)


READER_TEMPLATE_CON = (
    "You are the reader module of an agent brain. Use ONLY the consolidated memory, "
    "retrieved notes, and session transcripts below.\n\n"
    "Step 1 — For each block, write a short note listing facts relevant to the question.\n"
    "Step 2 — Reason over those notes to produce the final answer.\n"
    "Step 3 — Give a concise final answer with no extra speculation.\n\n"
    "{context}\n\nCurrent Date: {question_date}\n"
    "Question: {question}\n\nNotes (step 1):\nAnswer (steps 2–3):"
)
