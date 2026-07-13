"""LoCoMo honest recall through the NATIVE Rust CHORUS MaxSim+BM25 path.

Proves the engine (not a Python prototype) produces the number: ingests per-token
MiniLM vectors via `chorus_imprint_maxsim_batch` and recalls via
`chorus_recall_maxsim_batch` (ChorusField::recall_maxsim in Rust). Honest raw
recall@k, no neighbor expansion.

Frozen result: benchmarks/results/locomo-lateinteraction-engine-2026-07-13.json
  raw recall@150 = 96.9%  (beats the 96.3% Python prototype)

Requires:
  - a native build with the MaxSim bindings (cargo build -p fluctlightdb-native --release,
    installed as fluctlightdb_native)
  - the token cache benchmarks/locomo_lateinteraction.py builds (/tmp/locomo/cache/tok16.pkl)

Ingest feeds MaxSim tokens from bare turns (W=0) and BM25 content from ±2 context
bodies (W=2); duplicate dia rows get unique #s keys that parent_memory_id() maps
back to the dia for scoring.

Usage:
  PYTHONPATH=sdks/python FLUCTLIGHT_FABRIC=1 python benchmarks/locomo_engine_maxsim.py \
      --data /tmp/locomo/locomo10.json --json-out benchmarks/results/locomo-engine.json
"""
from __future__ import annotations

import argparse
import json
import os
import pickle
import sys
import time
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from bench_lanes import open_lane  # noqa: E402
from locomo_eval import collect_turns, normalize_evidence  # noqa: E402

CATS = {1: "multihop", 2: "temporal", 3: "opendomain", 4: "singlehop", 5: "adversarial"}
KS = [5, 10, 20, 50, 150]
DIM = 384


def flat_tokens(texts, tokcache):
    flat, counts = [], []
    for t in texts:
        T = tokcache[t[:800]].astype(np.float32)
        counts.append(T.shape[0])
        flat.append(T.ravel())
    return (np.concatenate(flat).astype(np.float32).tolist() if flat else []), counts


def main() -> int:
    ap = argparse.ArgumentParser(description="LoCoMo native CHORUS MaxSim+BM25 recall")
    ap.add_argument("--data", type=Path, default=Path("/tmp/locomo/locomo10.json"))
    ap.add_argument("--tokcache", type=Path, default=Path("/tmp/locomo/cache/tok16.pkl"))
    ap.add_argument("--w-bm", type=float, default=0.7)
    ap.add_argument("--json-out", type=Path, default=None)
    args = ap.parse_args()

    data = json.loads(args.data.read_text())
    if isinstance(data, dict):
        data = list(data.values())
    tokcache = pickle.load(args.tokcache.open("rb"))

    acc = {k: 0.0 for k in KS}
    nq = 0
    bycat: dict = {}
    hits150 = gold150 = 0
    t0 = time.time()
    for item in data:
        brain = open_lane("chorus")
        rows = collect_turns(item, "all", context_window=0)
        rows2 = collect_turns(item, "all", context_window=2)
        bodies0 = [r["body"][:800] for r in rows]
        bodies2 = [r["body"][:800] for r in rows2]
        seen: dict = {}
        keys = []
        for r in rows:
            d = r["dia"]
            c = seen.get(d, 0)
            seen[d] = c + 1
            keys.append(d if c == 0 else f"{d}#s{c}")
        tflat, tcounts = flat_tokens(bodies0, tokcache)
        brain._brain.chorus_imprint_maxsim_batch(keys, bodies2, keys, tflat, tcounts, DIM, 0.62)

        qas = [
            q for q in item.get("qa", [])
            if isinstance(q, dict) and (q.get("question") or "").strip() and q.get("evidence")
        ]
        cues = [(q.get("question") or "").strip()[:400] for q in qas]
        qflat, qcounts = flat_tokens(cues, tokcache)
        batch = brain._brain.chorus_recall_maxsim_batch(cues, qflat, qcounts, DIM, 150, args.w_bm)
        for qa, hits in zip(qas, batch):
            order = [h[0] for h in hits]
            ev = normalize_evidence(qa.get("evidence") or [])
            if not ev:
                continue
            nq += 1
            for k in KS:
                got = set(order[:k])
                acc[k] += sum(1 for e in ev if e in got) / len(ev)
            g = set(order[:150])
            h = sum(1 for e in ev if e in g)
            hits150 += h
            gold150 += len(ev)
            d = bycat.setdefault(qa.get("category"), [0, 0.0])
            d[0] += 1
            d[1] += h / len(ev)

    out = {
        "benchmark": "locomo",
        "protocol": "honest-raw-no-expansion",
        "lane": "chorus_engine",
        "retrieval": {"channel_1": "MaxSim (f16 tokens)", "channel_2": "BM25", "w_bm": args.w_bm},
        "conversations": len(data),
        "questions": nq,
        "wall_s": round(time.time() - t0, 1),
        "recall_at_k": {str(k): round(acc[k] / nq * 100, 2) for k in KS},
        "per_category_at_150": {CATS[c]: round(v[1] / v[0] * 100, 1) for c, v in sorted(bycat.items())},
        "span_hits_at_150": f"{hits150}/{gold150}",
    }
    print(json.dumps(out, indent=2))
    if args.json_out:
        args.json_out.write_text(json.dumps(out, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
