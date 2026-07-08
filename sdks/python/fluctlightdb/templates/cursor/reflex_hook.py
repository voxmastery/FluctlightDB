#!/usr/bin/env python3
"""Cursor post-turn hook — Reflex auto-ingest into FluctlightDB."""

from __future__ import annotations

import json
import sys
from pathlib import Path

# Hook stdin: {"user": "...", "assistant": "...", "tools": [{"name": "...", "result": "..."}]}
def main() -> int:
    try:
        from fluctlightdb import connect_agent
        from fluctlightdb.reflex import ReflexHook
    except ImportError:
        print("fluctlightdb[native] required for reflex hook", file=sys.stderr)
        return 0

    raw = sys.stdin.read()
    if not raw.strip():
        return 0
    payload = json.loads(raw)
    root = Path.cwd()
    brain_path = root / ".fluctlight" / "agents" / "cursor"
    brain = connect_agent(str(brain_path) if brain_path.is_dir() else None)
    hook = ReflexHook(brain)
    report = hook.after_turn(payload)
    print(json.dumps({"wm_pushed": report.wm_pushed, "tools": report.tools_observed}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
