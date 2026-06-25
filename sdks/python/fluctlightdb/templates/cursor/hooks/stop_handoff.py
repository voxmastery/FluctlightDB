#!/usr/bin/env python3
"""Cursor stop — write a brief handoff when the agent finishes."""

from __future__ import annotations

import json
import os
import sys


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
    return "Session ended — see recent edits and open tasks."


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

        pb = connect_project(agent="cursor")
        summary = _summary_from_hook(payload if isinstance(payload, dict) else {})
        pb.handoff(
            summary,
            status="paused",
            next_steps=["Continue from handoff context on next agent session."],
        )
        pb.checkpoint()
    except Exception:
        pass
    print("{}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
