#!/usr/bin/env python3
"""Cursor sessionEnd — checkpoint brains after a session."""

from __future__ import annotations

import json
import sys


def main() -> int:
    try:
        _ = sys.stdin.read()
        from fluctlightdb import connect_project

        connect_project(agent="cursor").checkpoint()
        print("{}")
    except Exception:
        print("{}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
