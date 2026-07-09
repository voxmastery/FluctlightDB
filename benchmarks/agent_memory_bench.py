#!/usr/bin/env python3
"""FluctlightDB Agent Memory Benchmark (FAMB).

Modern lanes:
  **agent** — ``connect_agent()`` unified recall (production path)
  **chorus** — CHORUS bulk imprint + GRG recall

Usage:
  PYTHONPATH=sdks/python python benchmarks/agent_memory_bench.py --mode agent
  PYTHONPATH=sdks/python python benchmarks/agent_memory_bench.py --mode chorus
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
SDK = REPO / "sdks/python"
if str(SDK) not in sys.path:
    sys.path.insert(0, str(SDK))

from bench_lanes import chorus_hits_to_ids, embed_minilm, open_lane  # noqa: E402

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

DETERMINISM_FACT = "user upgraded postgres to version 15 last tuesday"
DETERMINISM_CUE = "postgres upgrade version"

# Per-subtest sample sizes (disclosed in paper / JSON freeze files).
FAMB_SUITE_SIZES: dict[str, int] = {
    "paraphrase_recall_at_1": len(PARAPHRASE_PAIRS),
    "provenance_top1": 1,
    "persistence_recall": 1,
    "confusion_ingest": 1,
    "determinism": 1,
}


def _top_engram_id(brain: Any, cue: str, vec: Optional[list[float]] = None) -> Optional[str]:
    raw = brain.activate(cue, semantic_vector=vec, limit=3)
    recalls = raw.get("recalls") if isinstance(raw, dict) else raw
    if not recalls:
        return None
    return str(recalls[0].get("engram_id") or "")


def _top_chorus_id(brain: Any, cue: str, vec: Optional[list[float]] = None) -> Optional[str]:
    hits = brain.chorus_recall(cue, limit=3, semantic_vector=vec)
    ids = chorus_hits_to_ids(hits, 1)
    return ids[0] if ids else None


def _seed_noise_agent(brain: Any, n: int) -> None:
    for i in range(n):
        brain.experience(
            NOISE_SNIPPETS[i % len(NOISE_SNIPPETS)] + f" noise_{i}",
            context=f"noise:{i}",
            salience=0.2,
        )


def _seed_noise_chorus(brain: Any, n: int) -> None:
    batch = [
        {
            "memory_id": f"noise_{i}",
            "content": NOISE_SNIPPETS[i % len(NOISE_SNIPPETS)] + f" noise_{i}",
            "context": f"noise:{i}",
            "salience": 0.2,
        }
        for i in range(n)
    ]
    brain.chorus_imprint_batch(batch)


def suite_paraphrase_agent(brain: Any, *, noise: int) -> float:
    canon_ids: dict[str, str] = {}
    for content, _cue, vec in PARAPHRASE_PAIRS:
        rep = brain.experience(
            content,
            context="famb:canon",
            salience=0.78,
            semantic_vector=vec,
        )
        canon_ids[content] = str(rep.get("engram_id") or "")
    _seed_noise_agent(brain, noise)
    hits = 0
    for content, cue, vec in PARAPHRASE_PAIRS:
        top = _top_engram_id(brain, cue, vec)
        if top and top == canon_ids.get(content):
            hits += 1
    return hits / len(PARAPHRASE_PAIRS)


def suite_paraphrase_chorus(brain: Any, *, noise: int) -> float:
    texts = [p[0] for p in PARAPHRASE_PAIRS] + [p[1] for p in PARAPHRASE_PAIRS]
    vecs = embed_minilm(texts)
    canon_vecs = vecs[: len(PARAPHRASE_PAIRS)]
    cue_vecs = vecs[len(PARAPHRASE_PAIRS) :]
    batch = [
        {
            "memory_id": f"canon_{i}",
            "content": content,
            "context": "famb:canon",
            "semantic_vector": vec,
            "salience": 0.78,
        }
        for i, (content, vec) in enumerate(zip([p[0] for p in PARAPHRASE_PAIRS], canon_vecs))
    ]
    brain.chorus_imprint_batch(batch)
    _seed_noise_chorus(brain, noise)
    hits = 0
    for i, (content, cue, _vec) in enumerate(PARAPHRASE_PAIRS):
        top = _top_chorus_id(brain, cue, cue_vecs[i])
        if top == f"canon_{i}":
            hits += 1
        elif top and content in (content,):
            hits += 1
    return hits / len(PARAPHRASE_PAIRS)


def _chorus_sheath(
    *,
    verified: bool,
    provenance_kind: int = 0,
    source_uri: Optional[str] = None,
) -> dict[str, Any]:
    sheath: dict[str, Any] = {"verified": verified, "provenance_kind": provenance_kind}
    if source_uri:
        sheath["source_uri"] = source_uri
    return sheath


def suite_provenance_agent(brain: Any) -> float:
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


def _top_rag_doc_id(brain: Any, cue: str, vec: Optional[list[float]] = None) -> Optional[str]:
    raw = brain.activate(cue, semantic_vector=vec, limit=3)
    recalls = raw.get("recalls") if isinstance(raw, dict) else raw
    if not recalls:
        return None
    ep = recalls[0].get("episode") or {}
    rag = ep.get("rag") or {}
    doc = rag.get("doc_id")
    return str(doc) if doc else None


def suite_provenance_chorus(brain: Any) -> float:
    brain.chorus_imprint_batch(
        [
            {
                "memory_id": "ledger",
                "content": "ledger verified: agent wallet balance is $0.00 at level 1",
                "context": "ledger:wallet",
                "salience": 0.98,
                "sheath": _chorus_sheath(
                    verified=True,
                    provenance_kind=3,
                    source_uri="file://wallet.json",
                ),
            },
            {
                "memory_id": "chat",
                "content": "I think my wallet balance is $60 from yesterday's chat",
                "context": "chat:wallet",
                "salience": 0.35,
                "sheath": _chorus_sheath(verified=False, provenance_kind=0),
            },
        ]
    )
    # Promote verified ledger into hippocampus (CHORUS → durable engram path).
    brain.chorus_sleep()
    top = _top_rag_doc_id(brain, "what is my wallet balance")
    return 1.0 if top == "ledger" else 0.0


def suite_persistence_agent(mode: str) -> float:
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, "agent.brain")
        b1 = open_lane("agent", path)
        rep = b1.experience(
            "user prefers dark mode in all applications",
            context="prefs:ui",
            salience=0.8,
            semantic_vector=[0.77, 0.23, 0.0],
        )
        eid = str(rep.get("engram_id"))
        b1.checkpoint()
        b2 = open_lane("agent", path)
        top = _top_engram_id(b2, "does the user like dark mode", [0.77, 0.23, 0.0])
        return 1.0 if top == eid else 0.0


def suite_persistence_chorus(mode: str) -> float:
    with tempfile.TemporaryDirectory() as td:
        path = os.path.join(td, "chorus.brain")
        fact_vec, cue_vec = embed_minilm(
            [
                "user prefers dark mode in all applications",
                "does the user like dark mode",
            ]
        )
        b1 = open_lane("chorus", path)
        b1.chorus_imprint_batch(
            [
                {
                    "memory_id": "pref",
                    "content": "user prefers dark mode in all applications",
                    "context": "prefs:ui",
                    "semantic_vector": fact_vec,
                    "salience": 1.0,
                    "sheath": _chorus_sheath(verified=True, provenance_kind=4),
                }
            ]
        )
        b1.chorus_sleep()
        b1.checkpoint()
        b2 = open_lane("chorus", path)
        raw = b2.activate("does the user like dark mode", semantic_vector=cue_vec, limit=1)
        recalls = raw.get("recalls") if isinstance(raw, dict) else raw
        if not recalls:
            return 0.0
        eid = str(recalls[0].get("engram_id") or "")
        b3 = open_lane("chorus", path)
        raw2 = b3.activate("does the user like dark mode", semantic_vector=cue_vec, limit=1)
        recalls2 = raw2.get("recalls") if isinstance(raw2, dict) else raw2
        if not recalls2:
            return 0.0
        top = str(recalls2[0].get("engram_id") or "")
        return 1.0 if top == eid and eid and not eid.startswith("00000000-") else 0.0


def suite_confusion_agent(brain: Any) -> float:
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


def suite_confusion_chorus(brain: Any) -> float:
    fact_vec, cue_vec = embed_minilm(
        [
            "postmortem root cause: memory leak in the payment worker pod on node-3",
            "what was the root cause of the node-3 incident",
        ]
    )
    brain.chorus_imprint_batch(
        [
            {
                "memory_id": "noise_a",
                "content": "user mentioned node-3 heap charts looked odd during rollout",
                "context": "incident:1",
                "salience": 0.35,
            },
            {
                "memory_id": "noise_b",
                "content": "user mentioned node-3 heap charts looked odd during rollout in prod",
                "context": "incident:1b",
                "salience": 0.32,
            },
            {
                "memory_id": "root",
                "content": "postmortem root cause: memory leak in the payment worker pod on node-3",
                "context": "incident:root_cause",
                "semantic_vector": fact_vec,
                "salience": 0.9,
            },
        ]
    )
    top = _top_chorus_id(brain, "what was the root cause of the node-3 incident", cue_vec)
    return 1.0 if top == "root" else 0.0


def suite_determinism_agent(brain: Any) -> float:
    fact_vec, cue_vec = embed_minilm([DETERMINISM_FACT, DETERMINISM_CUE])
    brain.experience(
        DETERMINISM_FACT,
        context="db",
        salience=0.7,
        semantic_vector=fact_vec,
    )

    def _rank_ids() -> list[str]:
        raw = brain.activate(DETERMINISM_CUE, semantic_vector=cue_vec, limit=5)
        return [str(r.get("engram_id")) for r in (raw.get("recalls") or [])]

    a = _rank_ids()
    b = _rank_ids()
    return 1.0 if a == b and len(a) > 0 else 0.0


def suite_determinism_chorus(brain: Any) -> float:
    fact_vec, cue_vec = embed_minilm([DETERMINISM_FACT, DETERMINISM_CUE])
    brain.chorus_imprint_batch(
        [
            {
                "memory_id": "det",
                "content": DETERMINISM_FACT,
                "context": "db",
                "semantic_vector": fact_vec,
                "salience": 0.7,
            }
        ]
    )

    def _rank_ids() -> list[str]:
        return chorus_hits_to_ids(
            brain.chorus_recall(DETERMINISM_CUE, limit=5, semantic_vector=cue_vec), 5
        )

    a = _rank_ids()
    b = _rank_ids()
    return 1.0 if a == b and len(a) > 0 else 0.0


def run_famb(mode: str, *, noise: int) -> dict[str, Any]:
    t0 = time.perf_counter()
    lane = "agent_unified" if mode == "agent" else "chorus_grg"
    if mode == "agent":
        scores = {
            "paraphrase_recall_at_1": suite_paraphrase_agent(open_lane("agent"), noise=noise),
            "provenance_top1": suite_provenance_agent(open_lane("agent")),
            "persistence_recall": suite_persistence_agent(mode),
            "confusion_ingest": suite_confusion_agent(open_lane("agent")),
            "determinism": suite_determinism_agent(open_lane("agent")),
        }
    else:
        scores = {
            "paraphrase_recall_at_1": suite_paraphrase_chorus(open_lane("chorus"), noise=noise),
            "provenance_top1": suite_provenance_chorus(open_lane("chorus")),
            "persistence_recall": suite_persistence_chorus(mode),
            "confusion_ingest": suite_confusion_chorus(open_lane("chorus")),
            "determinism": suite_determinism_chorus(open_lane("chorus")),
        }
    macro = sum(scores.values()) / len(scores)
    return {
        "benchmark": "famb",
        "mode": mode,
        "lane": lane,
        "noise_distractors": noise,
        "suite_sizes": dict(FAMB_SUITE_SIZES),
        "note": "Internal regression suite (not peer benchmark); macro = mean of sub-scores.",
        "scores": scores,
        "macro": round(macro, 4),
        "wall_s": round(time.perf_counter() - t0, 2),
    }


def main() -> int:
    ap = argparse.ArgumentParser(description="FluctlightDB Agent Memory Benchmark (FAMB)")
    ap.add_argument("--mode", choices=("agent", "chorus"), default="agent")
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
