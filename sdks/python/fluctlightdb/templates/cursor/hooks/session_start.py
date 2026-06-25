#!/usr/bin/env python3
"""Cursor sessionStart — inject FluctlightDB project context."""

from __future__ import annotations

import json
import sys


def main() -> int:
    try:
        _ = sys.stdin.read()
        from fluctlightdb import connect_project

        ctx = connect_project(agent="cursor").session_context(limit=12)
        if ctx:
            print(json.dumps({"additional_context": ctx}))
        else:
            print("{}")
    except Exception:
        print("{}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
