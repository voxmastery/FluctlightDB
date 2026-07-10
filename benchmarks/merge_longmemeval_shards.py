#!/usr/bin/env python3
"""Merge sharded LongMemEval multi-K checkpoint JSONL files."""

from __future__ import annotations

import argparse
import json
from collections import defaultdict
from pathlib import Path


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("shards", nargs="+", type=Path)
    ap.add_argument("--json-out", type=Path, required=True)
    ap.add_argument("--report-ks", default="5,8,10")
    args = ap.parse_args()
    report_ks = [int(x) for x in args.report_ks.split(",") if x.strip()]

    rows: list[dict] = []
    seen: set[str] = set()
    for path in args.shards:
        if not path.is_file():
            continue
        with path.open() as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                row = json.loads(line)
                qid = str(row.get("question_id") or "")
                if qid in seen:
                    continue
                seen.add(qid)
                rows.append(row)

    by_type: dict[str, list[bool]] = defaultdict(list)
    for r in rows:
        by_type[str(r.get("question_type") or "unknown")].append(bool(r.get("hit_at_8", r.get("hit"))))

    summary: dict = {
        "benchmark": "longmemeval_s",
        "questions": len(rows),
        "report_ks": report_ks,
        "merged_from": [str(p) for p in args.shards],
    }
    for k in report_ks:
        key = f"hit_at_{k}"
        khits = sum(1 for r in rows if r.get(key))
        summary[f"session_recall_at_{k}"] = round(khits / len(rows), 4) if rows else 0.0
        summary[f"hits_at_{k}"] = f"{khits}/{len(rows)}"
    summary["by_type_at_8"] = {
        k: round(sum(v) / len(v), 4) for k, v in sorted(by_type.items())
    }

    out = {"summary": summary, "results": rows}
    args.json_out.parent.mkdir(parents=True, exist_ok=True)
    args.json_out.write_text(json.dumps(out, indent=2) + "\n")
    print(json.dumps(summary, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
