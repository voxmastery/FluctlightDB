#!/usr/bin/env python3
"""Track edited file paths during a Cursor session (session-local)."""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path


def main() -> int:
    raw = sys.stdin.read()
    try:
        payload = json.loads(raw) if raw.strip() else {}
    except json.JSONDecodeError:
        print("{}")
        return 0

    tool_input = payload.get("tool_input") or payload.get("input") or {}
    path = ""
    if isinstance(tool_input, dict):
        path = str(tool_input.get("path") or tool_input.get("file_path") or "")
    if not path:
        print("{}")
        return 0

    try:
        from fluctlightdb import connect_project

        pb = connect_project(agent="cursor")
        rel = path.replace("\\", "/")
        pb.remember(
            f"edited file: {rel}",
            scope="agent",
            context="session:files",
            salience=0.35,
        )
    except Exception:
        pass
    print("{}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
