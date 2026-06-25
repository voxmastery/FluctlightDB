#!/usr/bin/env python3
"""Cursor beforeSubmitPrompt — remind agent to recall project context."""

from __future__ import annotations

import json
import sys


def main() -> int:
    try:
        _ = sys.stdin.read()
        from fluctlightdb import connect_project

        ctx = connect_project(agent="cursor").session_context(limit=6)
        if ctx:
            print(json.dumps({"additional_context": "Before answering: check FluctlightDB memory.\n\n" + ctx[:4000]}))
        else:
            print("{}")
    except Exception:
        print("{}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
