#!/usr/bin/env python3
"""Reject withdrawn LoCoMo headlines in tracked public files.

Text files fail on these LoCoMo evidence-recall headlines:

- ``99.0%`` (also the TeX form ``99.0\\%``)
- ``0.99`` or a longer ``0.990…`` expansion, on a line that also mentions
  LoCoMo, evidence recall, or ``mean_evidence_recall``
- ``0.981`` or ``98.1%`` on such a line

JSON is parsed structurally. A number under ``mean_evidence_recall`` or inside
``recall_at_k`` fails when it is a withdrawn headline in either form:

- fraction: 0.99, 0.9902, 0.981
- percent: 99.0, 98.1

A text hit is allowed when that line contains ``withdrawn``, ``retract``, or
``superseded``. A JSON value is allowed when the object that contains it, or
the file root, has ``"status"`` ``superseded`` or ``retracted``, or a
``"retraction"`` string.

Not flagged: per-category cells such as temporal 98.1, timings such as 10.99
seconds, and nearby decimals such as 0.9925 or 0.9819.

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
MARKED_STATUS = {"superseded", "retracted"}
HEADLINE_KEYS = {"mean_evidence_recall", "recall_at_k"}


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


def line_fails(line: str) -> bool:
    if ALLOW.search(line):
        return False
    if ALWAYS.search(line):
        return True
    return bool(CONTEXT_NUMBER.search(line) and CONTEXT.search(line))


def object_marked(node: dict) -> bool:
    status = str(node.get("status", "")).lower()
    if status in MARKED_STATUS:
        return True
    retraction = node.get("retraction")
    return isinstance(retraction, str) and bool(retraction.strip())


def is_withdrawn_locomo_value(value: object) -> bool:
    """True for withdrawn headline magnitudes, fraction or percent."""
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return False
    number = float(value)
    if number >= 10:
        return abs(number - 99.0) <= 0.05 or abs(number - 98.1) <= 0.05
    return (
        abs(number - 0.99) <= 5e-4
        or abs(number - 0.9902) <= 5e-4
        or abs(number - 0.981) <= 5e-4
    )


def _format_hit(path: str, value: object) -> str:
    return f"{path}={value}"


def withdrawn_value_paths(data: object, *, marked: bool = False, prefix: str = "") -> list[str]:
    """Paths of withdrawn LoCoMo headline numbers not inside a marked object."""
    if isinstance(data, dict):
        here = marked or object_marked(data)
        hits: list[str] = []
        if not here:
            mean = data.get("mean_evidence_recall")
            if is_withdrawn_locomo_value(mean):
                key = f"{prefix}.mean_evidence_recall" if prefix else "mean_evidence_recall"
                hits.append(_format_hit(key, mean))
            recall = data.get("recall_at_k")
            recall_prefix = f"{prefix}.recall_at_k" if prefix else "recall_at_k"
            if isinstance(recall, dict):
                for bucket, value in recall.items():
                    if is_withdrawn_locomo_value(value):
                        hits.append(_format_hit(f"{recall_prefix}.{bucket}", value))
            elif is_withdrawn_locomo_value(recall):
                hits.append(_format_hit(recall_prefix, recall))
        for key, value in data.items():
            if key in HEADLINE_KEYS:
                continue
            child = f"{prefix}.{key}" if prefix else str(key)
            hits.extend(withdrawn_value_paths(value, marked=here, prefix=child))
        return hits
    if isinstance(data, list):
        hits = []
        for index, value in enumerate(data):
            child = f"{prefix}[{index}]" if prefix else f"[{index}]"
            hits.extend(withdrawn_value_paths(value, marked=marked, prefix=child))
        return hits
    if isinstance(data, str) and not marked and line_fails(data):
        return [f"{prefix}: {data.strip()[:160]}"]
    return []


def json_failures(text: str, rel: str) -> list[str] | None:
    """Structural failures, or None when the text is not a JSON document."""
    try:
        data = json.loads(text)
    except json.JSONDecodeError:
        return None
    return [f"{rel}: {hit}" for hit in withdrawn_value_paths(data)]


def tracked_public_files(root: Path) -> list[str]:
    out = subprocess.check_output(["git", "ls-files", "-z"], cwd=root)
    paths = [p.decode() for p in out.split(b"\0") if p]
    return [p for p in paths if is_public(p)]


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    failures: list[str] = []
    files = tracked_public_files(root)
    for rel in files:
        text = (root / rel).read_text(encoding="utf-8", errors="replace")
        if rel.endswith(".json"):
            structured = json_failures(text, rel)
            if structured is not None:
                failures.extend(structured)
                continue
        for number, line in enumerate(text.splitlines(), start=1):
            if line_fails(line):
                failures.append(f"{rel}:{number}: {line.strip()}")
    if failures:
        print("withdrawn LoCoMo headlines still presented as current:", file=sys.stderr)
        for item in failures:
            print(item, file=sys.stderr)
        return 1
    print(f"public LoCoMo claims ok ({len(files)} files)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
