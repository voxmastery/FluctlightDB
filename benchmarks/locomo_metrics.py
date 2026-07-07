"""LoCoMo evidence-recall metrics (gold dia_id in retrieved context)."""

from __future__ import annotations

import re
from typing import Any, Iterable


_DIA_RE = re.compile(r"\bD\d+:\d+\b")


def extract_dia_ids(text: str) -> set[str]:
    return set(_DIA_RE.findall(text or ""))


def recall_dia_ids(recalls: list[dict], limit: int = 150) -> set[str]:
    """Collect dia_ids from activate() recalls (rag doc_id, context, content)."""
    found: set[str] = set()
    for r in recalls[:limit]:
        ep = r.get("episode") or {}
        rag = ep.get("rag") or {}
        doc = rag.get("doc_id") or ep.get("doc_id")
        if doc and str(doc).startswith("D"):
            found.add(str(doc))
        ctx = ep.get("context") or ""
        found.update(extract_dia_ids(ctx))
        content = ep.get("content") or ""
        found.update(extract_dia_ids(content))
        if ctx.startswith("locomo:"):
            tail = ctx.split(":", 1)[-1]
            if tail.startswith("D"):
                found.add(tail)
    return found


def evidence_hit(evidence: Iterable[str], recalled: set[str]) -> bool:
    ev = {str(e).strip() for e in evidence if e}
    if not ev:
        return False
    return ev.issubset(recalled)


def evidence_recall_fraction(evidence: Iterable[str], recalled: set[str]) -> float:
    ev = [str(e).strip() for e in evidence if e]
    if not ev:
        return 0.0
    hits = sum(1 for e in ev if e in recalled)
    return hits / len(ev)


def summarize_hits(rows: list[dict[str, Any]]) -> dict[str, Any]:
    if not rows:
        return {"questions": 0, "mean_evidence_recall": 0.0, "evidence_all_in_context": 0.0}
    fracs = [float(r.get("evidence_frac") or 0.0) for r in rows]
    all_in = sum(1 for r in rows if r.get("all_evidence"))
    return {
        "questions": len(rows),
        "mean_evidence_recall": sum(fracs) / len(fracs),
        "evidence_all_in_context": all_in / len(rows),
        "evidence_hits": f"{sum(1 for f in fracs if f > 0)}/{len(rows)}",
    }
