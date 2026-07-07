#!/usr/bin/env python3
"""Pre-flight E2E validation gate — must pass before a fresh 500-question run."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
DEFAULT_DATA = Path("/tmp/longmemeval/data/longmemeval_s_cleaned.json")
DEFAULT_PRIOR = (
    REPO / "benchmarks/results/e2e-cert-paper-2026-07-07.checkpoint.jsonl"
)
TARGET_ACC = 0.98
MIN_VALIDATION_N = 40


def load_fail_ids(prior: Path) -> list[str]:
    if not prior.is_file():
        return []
    rows = {}
    for line in prior.read_text().splitlines():
        if line.strip():
            r = json.loads(line)
            rows[r["question_id"]] = r
    return [qid for qid, r in rows.items() if not r.get("autoeval_label")]


def sample_unrun_ids(data: list[dict], done: set[str], *, per_type: int = 8) -> list[str]:
    out: list[str] = []
    by_type: dict[str, list[str]] = {}
    for item in data:
        qid = str(item.get("question_id") or "")
        if qid in done:
            continue
        t = str(item.get("question_type") or "")
        by_type.setdefault(t, []).append(qid)
    for t in sorted(by_type):
        out.extend(by_type[t][:per_type])
    return out


def main() -> int:
    ap = argparse.ArgumentParser(description="Validate tuned E2E before fresh 500 run")
    ap.add_argument("--prior-checkpoint", type=Path, default=DEFAULT_PRIOR)
    ap.add_argument("--data", type=Path, default=DEFAULT_DATA)
    ap.add_argument("--target", type=float, default=TARGET_ACC)
    ap.add_argument("--sample-per-type", type=int, default=8)
    ap.add_argument("--llm-backend", default="openai")
    ap.add_argument("--json-out", type=Path, default=REPO / "benchmarks/results/e2e-validate-gate.json")
    args = ap.parse_args()

    data = json.loads(args.data.read_text())
    done = set()
    if args.prior_checkpoint.is_file():
        for line in args.prior_checkpoint.read_text().splitlines():
            if line.strip():
                done.add(json.loads(line)["question_id"])

    fail_ids = load_fail_ids(args.prior_checkpoint)
    sample_ids = sample_unrun_ids(data, done, per_type=args.sample_per_type)
    validate_ids = list(dict.fromkeys(fail_ids + sample_ids))
    if len(validate_ids) < MIN_VALIDATION_N:
        print(f"WARN: only {len(validate_ids)} validation questions (wanted >={MIN_VALIDATION_N})")

    print("E2E validation gate")
    print(f"  prior fails to re-test: {len(fail_ids)}")
    print(f"  unrun samples: {len(sample_ids)}")
    print(f"  total validation: {len(validate_ids)}")
    print(f"  target accuracy: {args.target:.0%}")

    cmd = [
        sys.executable,
        str(REPO / "benchmarks" / "longmemeval_e2e.py"),
        "--e2e-profile",
        "paper",
        "--llm-backend",
        args.llm_backend,
        "--question-ids",
        ",".join(validate_ids),
        "--json-out",
        str(args.json_out),
        "--checkpoint",
        str(args.json_out.with_suffix(".checkpoint.jsonl")),
    ]
    print("  running validation harness...")
    env = os.environ.copy()
    env["PYTHONPATH"] = f"{REPO / 'sdks' / 'python'}:{REPO / 'benchmarks'}"
    proc = subprocess.run(cmd, cwd=str(REPO), env=env)
    if proc.returncode != 0:
        print("FAIL: harness error")
        return 1

    if not args.json_out.is_file():
        print("FAIL: no output json")
        return 1

    payload = json.loads(args.json_out.read_text())
    results = {r["question_id"]: r for r in payload.get("results", [])}
    judged = [results[qid] for qid in validate_ids if qid in results]
    if not judged:
        print("FAIL: no judged results")
        return 1

    ok = sum(1 for r in judged if r.get("autoeval_label"))
    acc = ok / len(judged)
    by_type: dict[str, list[bool]] = {}
    for r in judged:
        by_type.setdefault(str(r.get("question_type")), []).append(bool(r.get("autoeval_label")))

    print(f"\n  validation accuracy: {acc:.1%} ({ok}/{len(judged)})")
    for t, vals in sorted(by_type.items()):
        print(f"    {t}: {sum(vals)/len(vals):.1%} ({len(vals)})")

    fail_rerun = [r for r in judged if r["question_id"] in fail_ids and not r.get("autoeval_label")]
    if fail_rerun:
        print(f"  still failing prior misses: {len(fail_rerun)}")
        for r in fail_rerun[:8]:
            print(f"    - {r['question_id']} ({r.get('question_type')})")

    if acc >= args.target:
        print(f"\nREADY for fresh 500 run (validation {acc:.1%} >= {args.target:.0%})")
        return 0
    print(f"\nNOT READY ({acc:.1%} < {args.target:.0%}) — tune more before fresh 500")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
