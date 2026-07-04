#!/usr/bin/env python3
"""Merge LongMemEval v4 full + e2e JSON into paper metrics freeze file."""

from __future__ import annotations

import argparse
import json
from datetime import date
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
DEFAULT_BASE = REPO / "benchmarks/results/paper-2026-07-04.json"


def load_json(path: Path) -> dict:
    return json.loads(path.read_text())


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--full", type=Path, required=True, help="longmemeval full v4 JSON")
    ap.add_argument("--e2e", type=Path, required=True, help="longmemeval e2e JSON")
    ap.add_argument("--base", type=Path, default=DEFAULT_BASE)
    ap.add_argument("--out", type=Path, default=None)
    args = ap.parse_args()

    base = load_json(args.base)
    full = load_json(args.full)
    e2e = load_json(args.e2e)
    fs = full.get("summary") or full
    es = e2e.get("summary") or e2e

    lme = base.setdefault("longmemeval_s", {})
    lme["session_recall_at_8"] = fs.get("session_recall_at_k")
    lme["hits"] = fs.get("hits")
    lme["by_type"] = fs.get("by_type")
    lme["questions"] = fs.get("questions", 500)
    lme["harness"] = fs.get("harness", "v4")
    lme["pref_facts_key"] = fs.get("pref_facts_key", True)
    lme["composite_note"] = None
    lme["unified_v4"] = True
    lme["wall_s_full"] = fs.get("wall_s")
    lme["sec_per_question_full"] = fs.get("sec_per_question")
    lme["frozen_source_full"] = str(args.full.name)
    lme["e2e"] = {
        "overall_accuracy": es.get("overall_accuracy"),
        "task_averaged_accuracy": es.get("task_averaged_accuracy"),
        "session_recall_at_k": es.get("session_recall_at_k"),
        "questions": es.get("questions"),
        "reader_model": es.get("reader_model"),
        "judge_model": es.get("judge_model"),
        "by_type_accuracy": es.get("by_type_accuracy"),
        "frozen_source": str(args.e2e.name),
    }

    out = args.out or REPO / f"benchmarks/results/paper-{date.today().isoformat()}.json"
    base["date"] = date.today().isoformat()
    base["harness"] = "FluctlightDB benchmark suite (paper v2)"
    out.write_text(json.dumps(base, indent=2) + "\n")
    print(f"Wrote {out}")
    print(
        f"Retrieval: {lme['hits']} ({lme['session_recall_at_8']:.1%}) | "
        f"E2E: {es.get('overall_accuracy', 0):.1%}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
