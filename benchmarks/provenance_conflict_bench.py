#!/usr/bin/env python3
"""Graded provenance-conflict benchmark for FluctlightDB (agent lane).

Each case ingests a verified ledger/tool fact and an unverified chat claim that
conflicts; the cue should rank the verified engram first.

Usage:
  PYTHONPATH=sdks/python python benchmarks/provenance_conflict_bench.py
  PYTHONPATH=sdks/python python benchmarks/provenance_conflict_bench.py --json-out benchmarks/results/provenance-conflict-2026-07-10.json
"""

from __future__ import annotations

import argparse
import json
import sys
import time
from pathlib import Path
from typing import Any, Optional

REPO = Path(__file__).resolve().parents[1]
SDK = REPO / "sdks/python"
if str(SDK) not in sys.path:
    sys.path.insert(0, str(SDK))

from bench_lanes import open_lane  # noqa: E402

# 10 domains × 5 numeric variants = 50 conflict cases.
_DOMAINS: list[tuple[str, str, str, str]] = [
    ("wallet", "ledger verified: account balance is ${v} USD", "chat guess: balance might be ${c} USD", "what is my account balance"),
    ("shipping", "tool output verified: order ships on ${v}", "user said in chat order ships ${c}", "when does my order ship"),
    ("subscription", "ledger verified: plan tier is ${v}", "chat rumor: user thinks tier is ${c}", "what subscription tier am i on"),
    ("api_quota", "tool grounded: api quota remaining ${v} calls", "chat claim: quota is ${c} calls left", "how much api quota remains"),
    ("inventory", "file observation verified: sku-42 stock count ${v}", "assistant guessed stock ${c} in chat", "what is sku-42 stock count"),
    ("meeting", "calendar verified: standup scheduled ${v}", "chat memory: standup was ${c}", "when is standup scheduled"),
    ("deployment", "ci verified: production version ${v}", "chat said version ${c} in slack", "what production version is deployed"),
    ("refund", "ledger verified: refund amount ${v} USD", "user chat: refund should be ${c}", "what is my refund amount"),
    ("license", "license server verified: seats in use ${v}", "chat thought seats were ${c}", "how many license seats are in use"),
    ("temperature", "sensor verified: server room temp ${v}C", "chat note: room temp ${c}C", "what is the server room temperature"),
]

_VARIANTS = [(10, 60), (0, 99), (250, 12), (42, 77), (1500, 200)]


def build_cases() -> list[dict[str, Any]]:
    out: list[dict[str, Any]] = []
    for domain, ledger_tpl, chat_tpl, cue in _DOMAINS:
        for vi, (verified_val, chat_val) in enumerate(_VARIANTS):
            case_id = f"{domain}_{vi}"
            out.append(
                {
                    "id": case_id,
                    "domain": domain,
                    "ledger_content": ledger_tpl.replace("${v}", str(verified_val)),
                    "chat_content": chat_tpl.replace("${c}", str(chat_val)),
                    "cue": cue,
                    "verified_value": verified_val,
                    "chat_value": chat_val,
                }
            )
    return out


def _top_id(brain: Any, cue: str, vec: Optional[list[float]]) -> tuple[Optional[str], Optional[str]]:
    raw = brain.activate(cue, semantic_vector=vec, limit=3)
    recalls = raw.get("recalls") if isinstance(raw, dict) else raw
    if not recalls:
        return None, None
    top = recalls[0]
    eid = str(top.get("engram_id") or "")
    content = str((top.get("episode") or {}).get("content") or "")
    return eid, content


def run_case(brain: Any, case: dict[str, Any], *, ledger_id: str) -> dict[str, Any]:
    cue = case["cue"]
    # Match FAMB provenance protocol: lexical activate without query embedding.
    top_id, top_content = _top_id(brain, cue, None)
    hit = top_id == ledger_id
    return {
        "id": case["id"],
        "hit": hit,
        "top_engram_id": top_id,
        "ledger_engram_id": ledger_id,
        "top_content_preview": (top_content or "")[:120],
    }


def run_suite(cases: list[dict[str, Any]], *, shared_brain: bool = False) -> dict[str, Any]:
    ledger_ids: dict[str, str] = {}
    if shared_brain:
        brain = open_lane("agent")
        for case in cases:
            rep = brain.experience(
                case["ledger_content"],
                context=f"ledger:{case['domain']}",
                salience=0.95,
                verified=True,
                provenance_kind="ledger_verified",
                source_uri=f"file://{case['domain']}.json",
                confidence=0.99,
            )
            eid = str(rep.get("engram_id") or "")
            brain.verify_fact(
                eid,
                provenance_kind="ledger_verified",
                source_uri=f"file://{case['domain']}.json",
                confidence=0.99,
            )
            ledger_ids[case["id"]] = eid
            brain.experience(
                case["chat_content"],
                context=f"chat:{case['domain']}",
                salience=0.35,
                verified=False,
                provenance_kind="chat_assertion",
                confidence=0.25,
            )
        rows = [run_case(brain, c, ledger_id=ledger_ids[c["id"]]) for c in cases]
    else:
        rows = []
        for case in cases:
            brain = open_lane("agent")
            rep = brain.experience(
                case["ledger_content"],
                context=f"ledger:{case['domain']}",
                salience=0.95,
                verified=True,
                provenance_kind="ledger_verified",
                source_uri=f"file://{case['domain']}.json",
                confidence=0.99,
            )
            eid = str(rep.get("engram_id") or "")
            brain.verify_fact(
                eid,
                provenance_kind="ledger_verified",
                source_uri=f"file://{case['domain']}.json",
                confidence=0.99,
            )
            brain.experience(
                case["chat_content"],
                context=f"chat:{case['domain']}",
                salience=0.35,
                verified=False,
                provenance_kind="chat_assertion",
                confidence=0.25,
            )
            rows.append(run_case(brain, case, ledger_id=eid))
    hits = sum(1 for r in rows if r["hit"])
    by_domain: dict[str, list[bool]] = {}
    for case, row in zip(cases, rows):
        by_domain.setdefault(case["domain"], []).append(row["hit"])
    domain_macro = {d: sum(v) / len(v) for d, v in by_domain.items()}
    return {
        "benchmark": "provenance_conflict",
        "lane": "agent",
        "condition": "shared_brain" if shared_brain else "isolated_brain",
        "n_cases": len(cases),
        "top1_accuracy": hits / len(cases) if cases else 0.0,
        "hits": f"{hits}/{len(cases)}",
        "by_domain_macro": domain_macro,
        "cases": rows,
    }


def main() -> int:
    ap = argparse.ArgumentParser(description="Provenance conflict benchmark (agent)")
    ap.add_argument(
        "--shared-brain",
        action="store_true",
        help="ingest all 50 conflict pairs into one brain (multi-tenant stress)",
    )
    ap.add_argument("--json-out", type=Path, default=None)
    args = ap.parse_args()
    cases = build_cases()
    t0 = time.perf_counter()
    out = run_suite(cases, shared_brain=args.shared_brain)
    out["wall_s"] = round(time.perf_counter() - t0, 2)
    print(json.dumps({k: v for k, v in out.items() if k != "cases"}, indent=2))
    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(json.dumps(out, indent=2) + "\n")
    if args.shared_brain:
        return 0
    return 0 if out["top1_accuracy"] >= 0.9 else 1


if __name__ == "__main__":
    raise SystemExit(main())
