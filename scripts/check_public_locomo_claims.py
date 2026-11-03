#!/usr/bin/env python3
"""Reject withdrawn LoCoMo headlines in tracked public files.

Fails when a public file still states these LoCoMo evidence-recall headlines:

- ``99.0%`` (also the TeX form ``99.0\\%``)
- ``0.99`` or a longer ``0.990…`` expansion of that same figure, on a line that
  also mentions LoCoMo, evidence recall, or ``mean_evidence_recall``
- ``0.981`` or ``98.1%`` on such a line (the early paper freezes stored the
  headline as ``mean_evidence_recall`` 0.981)

A hit is allowed when the same line contains ``withdrawn``, ``retract``, or
``superseded``, or when a JSON document is marked at the top level with
``"status": "superseded"`` / ``"retracted"`` or a ``"retraction"`` string.

Not flagged: per-category cells such as temporal 98.1 (no percent sign and no
headline field), timings such as 10.99 seconds, and nearby decimals such as
0.9925 or 0.9819.

Scanned paths (git-tracked only): README files, ``docs/**/*.md``,
``CHANGELOG.md``, ``MAINTAINER.md``, ``.zenodo.json``, ``CITATION.cff``,
``benchmarks/results/**/*.json``, and ``papers/`` / ``hub/`` site text
(``.md``, ``.html``, ``.tex``, ``.json``).

Run from the repo root: ``python3 scripts/check_public_locomo_claims.py``
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ALWAYS = re.compile(r"99\.0\s*\\?%")
CONTEXT_NUMBER = re.compile(
    r"(?<![\d.])(?:0\.99(?!\d)|0\.990\d*|0\.981(?!\d)|98\.1\s*\\?%)"
)
CONTEXT = re.compile(r"locomo|evidence[_\s-]*recall|mean_evidence_recall", re.I)
ALLOW = re.compile(r"withdrawn|retract|superseded", re.I)

PUBLIC_NAMES = {"README.md", "CHANGELOG.md", "MAINTAINER.md", "CITATION.cff", ".zenodo.json"}
SITE_SUFFIXES = (".md", ".html", ".tex", ".json")


def is_public(path: str) -> bool:
    name = path.rsplit("/", 1)[-1]
    if name in PUBLIC_NAMES:
        return True
    if path.startswith("docs/") and path.endswith(".md"):
        return True
    if path.startswith("benchmarks/results/") and path.endswith(".json"):
        return True
    if path.startswith(("papers/", "hub/")) and path.endswith(SITE_SUFFIXES):
        return True
    return False


def json_marked(text: str) -> bool:
    try:
        data = json.loads(text)
    except json.JSONDecodeError:
        return False
    if not isinstance(data, dict):
        return False
    status = str(data.get("status", "")).lower()
    if status in {"superseded", "retracted"}:
        return True
    retraction = data.get("retraction")
    return isinstance(retraction, str) and bool(retraction.strip())


def line_fails(line: str) -> bool:
    if ALLOW.search(line):
        return False
    if ALWAYS.search(line):
        return True
    return bool(CONTEXT_NUMBER.search(line) and CONTEXT.search(line))


def tracked_public_files(root: Path) -> list[str]:
    out = subprocess.check_output(["git", "ls-files", "-z"], cwd=root)
    paths = [p.decode() for p in out.split(b"\0") if p]
    return [p for p in paths if is_public(p)]


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    failures: list[str] = []
    for rel in tracked_public_files(root):
        path = root / rel
        text = path.read_text(encoding="utf-8", errors="replace")
        if json_marked(text):
            continue
        for number, line in enumerate(text.splitlines(), start=1):
            if line_fails(line):
                failures.append(f"{rel}:{number}: {line.strip()}")
    if failures:
        print("withdrawn LoCoMo headlines still presented as current:", file=sys.stderr)
        for item in failures:
            print(item, file=sys.stderr)
        return 1
    print(f"public LoCoMo claims ok ({len(tracked_public_files(root))} files)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
