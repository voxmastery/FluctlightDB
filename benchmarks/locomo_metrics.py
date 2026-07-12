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
    """Summarize rows. Emits raw_* and expanded_*; primary keys stay expanded for legacy freezes."""
    if not rows:
        return {
            "questions": 0,
            "mean_evidence_recall": 0.0,
            "evidence_all_in_context": 0.0,
            "mean_evidence_recall_raw": 0.0,
            "evidence_all_in_context_raw": 0.0,
            "mean_evidence_recall_expanded": 0.0,
            "evidence_all_in_context_expanded": 0.0,
        }

    def _fracs(expanded: bool) -> list[float]:
        out: list[float] = []
        for r in rows:
            if expanded:
                v = r.get("evidence_frac_expanded")
                if v is None:
                    v = r.get("evidence_frac")
            else:
                v = r.get("evidence_frac_raw")
                if v is None:
                    v = r.get("evidence_frac")
            out.append(float(v or 0.0))
        return out

    def _all_rate(expanded: bool) -> float:
        n = 0
        for r in rows:
            if expanded:
                v = r.get("all_evidence_expanded")
                if v is None:
                    v = r.get("all_evidence")
            else:
                v = r.get("all_evidence_raw")
                if v is None:
                    v = r.get("all_evidence")
            if v:
                n += 1
        return n / len(rows)

    raw_fracs = _fracs(False)
    exp_fracs = _fracs(True)
    raw_mean = sum(raw_fracs) / len(raw_fracs)
    exp_mean = sum(exp_fracs) / len(exp_fracs)
    return {
        "questions": len(rows),
        "mean_evidence_recall": exp_mean,
        "evidence_all_in_context": _all_rate(True),
        "evidence_hits": f"{sum(1 for f in exp_fracs if f > 0)}/{len(rows)}",
        "mean_evidence_recall_raw": raw_mean,
        "evidence_all_in_context_raw": _all_rate(False),
        "mean_evidence_recall_expanded": exp_mean,
        "evidence_all_in_context_expanded": _all_rate(True),
        "scoring_note": (
            "expanded = retrieved dia_ids plus expand_session_neighbors(window); "
            "raw = retrieved dia_ids only (stricter). "
            "mean_evidence_recall aliases expanded for freeze compatibility."
        ),
    }
