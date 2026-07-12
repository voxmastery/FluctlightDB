#!/usr/bin/env python3
"""Cursor stop — write a git-aware handoff when the agent finishes."""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path


def _summary_from_hook(payload: dict) -> str:
    for key in ("summary", "status_message", "message", "text"):
        val = payload.get(key)
        if isinstance(val, str) and val.strip():
            return val.strip()[:1200]
    conv = payload.get("conversation") or payload.get("messages")
    if isinstance(conv, list) and conv:
        for item in reversed(conv):
            if not isinstance(item, dict):
                continue
            text = item.get("text") or item.get("content")
            if isinstance(text, str) and text.strip():
                return text.strip()[:1200]
    return ""


def main() -> int:
    raw = sys.stdin.read()
    try:
        payload = json.loads(raw) if raw.strip() else {}
    except json.JSONDecodeError:
        payload = {}

    if os.environ.get("FLUCTLIGHT_SKIP_STOP_HANDOFF", "").lower() in ("1", "true", "yes"):
        print("{}")
        return 0

    try:
        from fluctlightdb import connect_project
        from fluctlightdb.handoff_capture import capture_session_summary

        pb = connect_project(agent="cursor")
        hook_summary = _summary_from_hook(payload if isinstance(payload, dict) else {})
        cap = capture_session_summary(Path(pb.config.root), hook_summary=hook_summary)
        pb.handoff(
            cap.summary,
            status="paused",
            next_steps=cap.next_steps,
            files=cap.files,
        )
        pb.checkpoint()
    except Exception:
        pass
    print("{}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
