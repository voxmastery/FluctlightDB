"""Merge BM25 vs Fluctlight LongMemEval runs into hard-slice + claim gates."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any, Optional, Sequence


GBRAIN_RECALL_AT_5 = 0.976  # published peer record to beat


def hit_at(row: dict[str, Any], k: int) -> bool:
    key = f"hit_at_{k}"
    if key in row:
        return bool(row[key])
    if k == 8 and "hit" in row:
        return bool(row["hit"])
    return False


def summarize_ks(rows: Sequence[dict[str, Any]], ks: Sequence[int]) -> dict[str, Any]:
    n = len(rows)
    out: dict[str, Any] = {"questions": n}
    for k in ks:
        khits = sum(1 for r in rows if hit_at(r, k))
        out[f"session_recall_at_{k}"] = round(khits / n, 4) if n else 0.0
        out[f"hits_at_{k}"] = f"{khits}/{n}"
    return out


def build_hard_slice(
    bm25_rows: Sequence[dict[str, Any]],
    *,
    k: int = 8,
    fallback_k: Optional[int] = 5,
) -> list[str]:
    """Freeze H = question_ids where BM25 misses at k (fallback to fallback_k if empty)."""
    misses = [
        str(r["question_id"])
        for r in bm25_rows
        if r.get("question_id") is not None and not hit_at(r, k)
    ]
    if misses or fallback_k is None:
        return sorted(misses)
    return sorted(
        str(r["question_id"])
        for r in bm25_rows
        if r.get("question_id") is not None and not hit_at(r, fallback_k)
    )


def _index_by_qid(rows: Sequence[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    return {str(r["question_id"]): r for r in rows if r.get("question_id") is not None}


def claim_gates(
    *,
    fluctlight_recall_at_5: float,
    bm25_recall_at_8: float,
    fluctlight_recall_at_8: float,
    hard_slice_n: int,
    hard_slice_hits_fl: int,
    hard_slice_hits_bm25: int,
    preference_hits: int,
    preference_n: int,
) -> dict[str, Any]:
    """Spec gates: @5 > gbrain, hard-slice absolute win, preference 30/30."""
    official = fluctlight_recall_at_5 > GBRAIN_RECALL_AT_5
    abs_gain = hard_slice_hits_fl - hard_slice_hits_bm25
    hard_ok = hard_slice_n > 0 and abs_gain >= 1
    if hard_slice_n >= 10:
        fl_rate = hard_slice_hits_fl / hard_slice_n
        bm_rate = hard_slice_hits_bm25 / hard_slice_n
        hard_ok = hard_ok and (fl_rate - bm_rate) >= 0.10
    pref_ok = preference_n == 30 and preference_hits == 30
    return {
        "official_record_at_5": official,
        "fluctlight_recall_at_5": fluctlight_recall_at_5,
        "gbrain_bar": GBRAIN_RECALL_AT_5,
        "hard_slice_win": hard_ok,
        "hard_slice_absolute_gain": abs_gain,
        "preference_30_30": pref_ok,
        "preference_hits": preference_hits,
        "preference_n": preference_n,
        "aggregate_at_8_note": {
            "bm25": bm25_recall_at_8,
            "fluctlight": fluctlight_recall_at_8,
            "saturated_ceiling": True,
        },
        "all_pass": official and hard_ok and pref_ok,
    }


def compare_runs(
    bm25_rows: Sequence[dict[str, Any]],
    fluctlight_rows: Sequence[dict[str, Any]],
    *,
    ks: Sequence[int] = (1, 3, 5, 8),
    hard_k: int = 8,
) -> dict[str, Any]:
    bm25_i = _index_by_qid(bm25_rows)
    fl_i = _index_by_qid(fluctlight_rows)
    shared = sorted(set(bm25_i) & set(fl_i))
    bm25_shared = [bm25_i[q] for q in shared]
    fl_shared = [fl_i[q] for q in shared]

    H = build_hard_slice(bm25_shared, k=hard_k, fallback_k=5)
    bm25_h = [bm25_i[q] for q in H if q in bm25_i]
    fl_h = [fl_i[q] for q in H if q in fl_i]

    bm25_sum = summarize_ks(bm25_shared, ks)
    fl_sum = summarize_ks(fl_shared, ks)

    pref_fl = [r for r in fl_shared if r.get("question_type") == "single-session-preference"]
    pref_hits = sum(1 for r in pref_fl if hit_at(r, 8))

    hs_fl = sum(1 for r in fl_h if hit_at(r, hard_k))
    hs_bm = sum(1 for r in bm25_h if hit_at(r, hard_k))
    hs_n = len(H)
    hs_fl_rate = (hs_fl / hs_n) if hs_n else 0.0
    hs_bm_rate = (hs_bm / hs_n) if hs_n else 0.0

    gates = claim_gates(
        fluctlight_recall_at_5=fl_sum.get("session_recall_at_5", 0.0),
        bm25_recall_at_8=bm25_sum.get("session_recall_at_8", 0.0),
        fluctlight_recall_at_8=fl_sum.get("session_recall_at_8", 0.0),
        hard_slice_n=hs_n,
        hard_slice_hits_fl=hs_fl,
        hard_slice_hits_bm25=hs_bm,
        preference_hits=pref_hits,
        preference_n=len(pref_fl),
    )

    return {
        "questions_shared": len(shared),
        "bm25": bm25_sum,
        "fluctlight": fl_sum,
        "hard_slice_ids": H,
        "hard_slice_n": hs_n,
        "hard_slice_recall_at_8": {
            "bm25": round(hs_bm_rate, 4),
            "fluctlight": round(hs_fl_rate, 4),
            "hits_bm25": f"{hs_bm}/{hs_n}",
            "hits_fluctlight": f"{hs_fl}/{hs_n}",
        },
        "hard_slice_delta_at_8": round(hs_fl_rate - hs_bm_rate, 4),
        "preference": {
            "hits_at_8": pref_hits,
            "n": len(pref_fl),
            "session_recall_at_8": round(pref_hits / len(pref_fl), 4) if pref_fl else 0.0,
            "miss_ids": [str(r["question_id"]) for r in pref_fl if not hit_at(r, 8)],
        },
        "claim_gates": gates,
    }


def load_run(path: Path) -> list[dict[str, Any]]:
    data = json.loads(path.read_text())
    if isinstance(data, dict) and "results" in data:
        return list(data["results"])
    if isinstance(data, list):
        return data
    raise ValueError(f"unrecognized run JSON shape: {path}")


def write_hard_slice_freeze(
    out: Path,
    *,
    hard_slice_ids: list[str],
    bm25_path: str,
    meta: Optional[dict[str, Any]] = None,
) -> None:
    payload = {
        "frozen": True,
        "metric": "bm25_session_recall_miss_at_8_or_fallback_5",
        "hard_slice_ids": list(hard_slice_ids),
        "hard_slice_n": len(hard_slice_ids),
        "bm25_source": bm25_path,
        "meta": meta or {},
    }
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(payload, indent=2) + "\n")


def main(argv: Optional[Sequence[str]] = None) -> int:
    ap = argparse.ArgumentParser(description="BM25 vs Fluctlight LongMemEval scoreboard")
    ap.add_argument("--bm25", type=Path, required=True)
    ap.add_argument("--fluctlight", type=Path, required=True)
    ap.add_argument("--ks", default="1,3,5,8")
    ap.add_argument("--hard-slice-out", type=Path, default=None)
    ap.add_argument("--json-out", type=Path, default=None)
    args = ap.parse_args(argv)
    ks = tuple(int(x) for x in args.ks.split(",") if x.strip())
    cmp = compare_runs(load_run(args.bm25), load_run(args.fluctlight), ks=ks)
    if args.hard_slice_out:
        write_hard_slice_freeze(
            args.hard_slice_out,
            hard_slice_ids=cmp["hard_slice_ids"],
            bm25_path=str(args.bm25),
            meta={"claim_gates": cmp["claim_gates"]},
        )
    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(json.dumps(cmp, indent=2) + "\n")
    print(json.dumps(cmp, indent=2))
    return 0 if cmp["claim_gates"]["all_pass"] else 2


if __name__ == "__main__":
    raise SystemExit(main())
