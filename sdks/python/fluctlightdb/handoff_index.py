"""Append-only handoff inbox index (deterministic cross-agent handoffs)."""

from __future__ import annotations

import json
import os
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Optional

from .handoff import Handoff
from .lock import file_write_lock
from .platform import normalize_subdir

INDEX_NAME = "handoffs.jsonl"
INDEX_LOCK = ".handoffs.lock"


def index_path(fluctlight_root: Path) -> Path:
    return fluctlight_root / INDEX_NAME


def _index_lock_path(fluctlight_root: Path) -> Path:
    return fluctlight_root / INDEX_LOCK


def append_handoff(fluctlight_root: Path, handoff: Handoff) -> None:
    """Append handoff to JSONL index (atomic under lock)."""
    fluctlight_root.mkdir(parents=True, exist_ok=True)
    line = json.dumps(
        {
            "handoff_id": handoff.handoff_id,
            "agent": handoff.agent,
            "subdir": normalize_subdir(handoff.subdir),
            "session_id": handoff.session_id,
            "status": handoff.status,
            "summary": handoff.summary,
            "next_steps": handoff.next_steps,
            "files": handoff.files,
            "created_at": handoff.created_at,
        },
        ensure_ascii=False,
        separators=(",", ":"),
    )
    path = index_path(fluctlight_root)
    with file_write_lock(_index_lock_path(fluctlight_root), timeout_s=15.0):
        with open(path, "a", encoding="utf-8") as fh:
            fh.write(line + "\n")
            fh.flush()
            os.fsync(fh.fileno())


def _parse_created_at(value: str) -> datetime:
    if not value:
        return datetime.min.replace(tzinfo=timezone.utc)
    try:
        dt = datetime.fromisoformat(value.replace("Z", "+00:00"))
        if dt.tzinfo is None:
            dt = dt.replace(tzinfo=timezone.utc)
        return dt
    except ValueError:
        return datetime.min.replace(tzinfo=timezone.utc)


def _matches_since(created_at: str, since: Optional[str]) -> bool:
    if not since:
        return True
    try:
        cutoff = _parse_created_at(since)
        return _parse_created_at(created_at) >= cutoff
    except ValueError:
        return True


def list_handoffs(
    fluctlight_root: Path,
    *,
    agent: Optional[str] = None,
    subdir: Optional[str] = None,
    status: Optional[str] = None,
    since: Optional[str] = None,
    limit: int = 20,
) -> list[Handoff]:
    path = index_path(fluctlight_root)
    if not path.is_file():
        return []
    rows: list[Handoff] = []
    norm_sub = normalize_subdir(subdir) if subdir is not None else None
    agent_l = agent.lower() if agent else None
    status_l = status.lower() if status else None
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            try:
                data = json.loads(line)
                h = Handoff(
                    agent=str(data.get("agent", "")),
                    subdir=str(data.get("subdir", "")),
                    session_id=str(data.get("session_id", "")),
                    status=str(data.get("status", "paused")),
                    summary=str(data.get("summary", "")),
                    next_steps=list(data.get("next_steps") or []),
                    files=list(data.get("files") or []),
                    handoff_id=str(data.get("handoff_id", "")),
                    created_at=str(data.get("created_at", "")),
                )
            except (json.JSONDecodeError, TypeError):
                continue
            if agent_l and h.agent.lower() != agent_l:
                continue
            if norm_sub is not None and normalize_subdir(h.subdir) != norm_sub:
                continue
            if status_l and h.status.lower() != status_l:
                continue
            if not _matches_since(h.created_at, since):
                continue
            rows.append(h)
    rows.sort(key=lambda h: _parse_created_at(h.created_at), reverse=True)
    return rows[: max(1, limit)]


def get_handoff(fluctlight_root: Path, handoff_id: str) -> Optional[Handoff]:
    for h in list_handoffs(fluctlight_root, limit=10_000):
        if h.handoff_id == handoff_id:
            return h
    return None
