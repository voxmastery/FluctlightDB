#!/usr/bin/env python3
"""FluctlightDB Agent Memory Benchmark (FAMB).

Tests agent-specific memory behaviors BEIR does not cover:
  paraphrase_recall@1, provenance_top1, persistence_recall,
  confusion_ingest, determinism.

Usage:
  PYTHONPATH=sdks/python python benchmarks/agent_memory_bench.py --mode agent
  PYTHONPATH=sdks/python python benchmarks/agent_memory_bench.py --mode index
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
import time
from pathlib import Path
from typing import Any, Optional

REPO = Path(__file__).resolve().parents[1]
SDK = REPO / "sdks" / "python"
if str(SDK) not in sys.path:
    sys.path.insert(0, str(SDK))

from fluctlightdb.brain import FluctlightBrain  # noqa: E402

PARAPHRASE_PAIRS: list[tuple[str, str, list[float]]] = [
    ("database connection pool exhausted", "db pool timeout", [0.9, 0.1, 0.0]),
    ("redis cache miss storm", "cache invalidation spike", [0.85, 0.15, 0.0]),
    ("kubernetes pod crash loop", "k8s container restart loop", [0.88, 0.12, 0.0]),
    ("payment webhook signature invalid", "stripe webhook auth failed", [0.92, 0.08, 0.0]),
    ("user login brute force", "account lockout threshold", [0.8, 0.2, 0.0]),
    ("nginx upstream timeout", "reverse proxy gateway timeout", [0.87, 0.13, 0.0]),
    ("postgres replication lag", "db replica delay high", [0.91, 0.09, 0.0]),
    ("s3 upload multipart failure", "object storage upload aborted", [0.86, 0.14, 0.0]),
    ("graphql query complexity limit", "api query cost exceeded", [0.84, 0.16, 0.0]),
    ("mqtt broker disconnect storm", "iot broker connection drop", [0.83, 0.17, 0.0]),
]

NOISE_SNIPPETS = [
    "user asked about weather in seattle next week",
    "assistant recommended a pasta recipe with basil",
    "user mentioned their cat likes tuna treats",
    "discussion about quarterly sales targets in emea",
    "user booked flights to chicago for a conference",
]


def _open_brain(mode: str, path: Optional[str] = None) -> FluctlightBrain:
    if mode == "index":
        return FluctlightBrain.connect_index(path) if path else FluctlightBrain.connect_index()
    return FluctlightBrain.connect(path) if path else FluctlightBrain.new()


def _top_engram_id(brain: FluctlightBrain, cue: str, vec: Optional[list[float]] = None) -> Optional[str]:
    raw = brain.activate(cue, semantic_vector=vec, limit=3)
    recalls = raw.get("recalls") if isinstance(raw, dict) else raw
    if not recalls:
        return None
    return str(recalls[0].get("engram_id") or "")


def _seed_noise(brain: FluctlightBrain, n: int) -> None:
    for i in range(n):
        brain.experience(
            NOISE_SNIPPETS[i % len(NOISE_SNIPPETS)] + f" noise_{i}",
            context=f"noise:{i}",
            salience=0.2,
        )


def suite_paraphrase(brain: FluctlightBrain, *, noise: int) -> float:
    canon_ids: dict[str, str] = {}
    for content, _cue, vec in PARAPHRASE_PAIRS:
        rep = brain.experience(
            content,
            context="famb:canon",
            salience=0.78,
            semantic_vector=vec,
        )
        canon_ids[content] = str(rep.get("engram_id") or "")
    _seed_noise(brain, noise)
    hits = 0
    for content, cue, vec in PARAPHRASE_PAIRS:
        top = _top_engram_id(brain, cue, vec)
        if top and top == canon_ids.get(content):
            hits += 1
    return hits / len(PARAPHRASE_PAIRS)


def suite_provenance(brain: FluctlightBrain) -> float:
    ledger = brain.experience(
        "ledger verified: agent wallet balance is $0.00 at level 1",
        context="ledger:wallet",
        salience=0.98,
        verified=True,
        provenance_kind="ledger_verified",
        source_uri="file://wallet.json",
        confidence=0.99,
    )
    brain.verify_fact(
        str(ledger.get("engram_id")),
        provenance_kind="ledger_verified",
        source_uri="file://wallet.json",
        confidence=0.99,
    )
    brain.experience(
        "I think my wallet balance is $60 from yesterday's chat",
        context="chat:wallet",
        salience=0.35,
        verified=False,
        provenance_kind="chat_assertion",
        confidence=0.3,
    )
    top = _top_engram_id(brain, "what is my wallet balance")
    return 1.0 if top == str(ledger.get("engram_id")) else 0.0


def suite_persistence(mode: str) -> float:
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, "agent.brain")
        b1 = _open_brain(mode, path)
        rep = b1.experience(
            "user prefers dark mode in all applications",
            context="prefs:ui",
            salience=0.8,
            semantic_vector=[0.77, 0.23, 0.0],
        )
        eid = str(rep.get("engram_id"))
        b1.checkpoint()
        b2 = _open_brain(mode, path)
        top = _top_engram_id(b2, "does the user like dark mode", [0.77, 0.23, 0.0])
        return 1.0 if top == eid else 0.0


def suite_confusion(brain: FluctlightBrain) -> float:
    brain.experience(
        "user mentioned node-3 heap charts looked odd during rollout",
        context="incident:1",
        salience=0.35,
    )
    brain.experience(
        "user mentioned node-3 heap charts looked odd during rollout in prod",
        context="incident:1b",
        salience=0.32,
    )
    fact = brain.experience(
        "postmortem root cause: memory leak in the payment worker pod on node-3",
        context="incident:root_cause",
        salience=0.9,
        semantic_vector=[0.7, 0.25, 0.05],
    )
    fid = str(fact.get("engram_id"))
    top = _top_engram_id(
        brain, "what was the root cause of the node-3 incident", [0.7, 0.25, 0.05]
    )
    return 1.0 if top == fid else 0.0


def suite_determinism(brain: FluctlightBrain) -> float:
    brain.experience("user upgraded postgres to version 15 last tuesday", context="db", salience=0.7)
    a = [
        str(r.get("engram_id"))
        for r in (brain.activate("postgres upgrade version", limit=5).get("recalls") or [])
    ]
    b = [
        str(r.get("engram_id"))
        for r in (brain.activate("postgres upgrade version", limit=5).get("recalls") or [])
    ]
    return 1.0 if a == b and len(a) > 0 else 0.0


def run_famb(mode: str, *, noise: int) -> dict[str, Any]:
    t0 = time.perf_counter()
    brain = _open_brain(mode)
    scores = {
        "paraphrase_recall_at_1": suite_paraphrase(brain, noise=noise),
        "provenance_top1": suite_provenance(_open_brain(mode)),
        "persistence_recall": suite_persistence(mode),
        "confusion_ingest": suite_confusion(_open_brain(mode)),
        "determinism": suite_determinism(_open_brain(mode)),
    }
    macro = sum(scores.values()) / len(scores)
    return {
        "benchmark": "famb",
        "mode": mode,
        "noise_distractors": noise,
        "scores": scores,
        "macro": round(macro, 4),
        "wall_s": round(time.perf_counter() - t0, 2),
    }


def main() -> int:
    ap = argparse.ArgumentParser(description="FluctlightDB Agent Memory Benchmark (FAMB)")
    ap.add_argument("--mode", choices=("agent", "index"), default="agent")
    ap.add_argument("--noise", type=int, default=int(os.environ.get("FAMB_NOISE", "200")))
    ap.add_argument("--json-out", type=Path, default=None)
    args = ap.parse_args()
    out = run_famb(args.mode, noise=args.noise)
    print(json.dumps(out, indent=2))
    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(json.dumps(out, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
