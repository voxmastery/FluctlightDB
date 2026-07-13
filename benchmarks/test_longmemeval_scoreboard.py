# benchmarks/test_longmemeval_scoreboard.py
from __future__ import annotations

from longmemeval_scoreboard import (
    build_hard_slice,
    claim_gates,
    compare_runs,
    hit_at,
    summarize_ks,
)


def _row(qid: str, qtype: str, **hits: bool) -> dict:
    row = {"question_id": qid, "question_type": qtype, "hit": hits.get("hit_at_8", False)}
    for k, v in hits.items():
        row[k] = v
    return row


def test_hit_at_prefers_explicit_hit_at_k():
    row = _row("a", "temporal-reasoning", hit_at_5=True, hit_at_8=True)
    assert hit_at(row, 5) is True
    assert hit_at(row, 1) is False  # missing key => False


def test_build_hard_slice_bm25_miss_at_8():
    bm25 = [
        _row("m1", "temporal-reasoning", hit_at_8=False, hit_at_5=False),
        _row("h1", "multi-session", hit_at_8=True, hit_at_5=True),
        _row("m2", "single-session-preference", hit_at_8=False, hit_at_5=False),
    ]
    H = build_hard_slice(bm25, k=8)
    assert H == ["m1", "m2"]


def test_build_hard_slice_fallback_to_k5_when_empty_at_8():
    bm25 = [
        _row("x", "temporal-reasoning", hit_at_8=True, hit_at_5=False),
        _row("y", "multi-session", hit_at_8=True, hit_at_5=True),
    ]
    H = build_hard_slice(bm25, k=8, fallback_k=5)
    assert H == ["x"]


def test_compare_runs_hard_slice_delta():
    bm25 = [
        _row("m1", "temporal-reasoning", hit_at_8=False),
        _row("m2", "multi-session", hit_at_8=False),
        _row("e1", "knowledge-update", hit_at_8=True),
    ]
    fl = [
        _row("m1", "temporal-reasoning", hit_at_8=True),
        _row("m2", "multi-session", hit_at_8=False),
        _row("e1", "knowledge-update", hit_at_8=True),
    ]
    cmp = compare_runs(bm25, fl, ks=(1, 3, 5, 8))
    assert cmp["hard_slice_ids"] == ["m1", "m2"]
    assert cmp["hard_slice_recall_at_8"]["bm25"] == 0.0
    assert cmp["hard_slice_recall_at_8"]["fluctlight"] == 0.5
    assert cmp["hard_slice_delta_at_8"] == 0.5


def test_claim_gates_world_record_package():
    gates = claim_gates(
        fluctlight_recall_at_5=0.978,
        bm25_recall_at_8=0.976,
        fluctlight_recall_at_8=0.976,
        hard_slice_n=12,
        hard_slice_hits_fl=8,
        hard_slice_hits_bm25=0,
        preference_hits=30,
        preference_n=30,
    )
    assert gates["official_record_at_5"] is True
    assert gates["hard_slice_win"] is True
    assert gates["preference_30_30"] is True
    assert gates["all_pass"] is True


def test_claim_gates_fail_when_at5_tied_with_gbrain():
    gates = claim_gates(
        fluctlight_recall_at_5=0.976,
        bm25_recall_at_8=0.976,
        fluctlight_recall_at_8=0.976,
        hard_slice_n=10,
        hard_slice_hits_fl=10,
        hard_slice_hits_bm25=0,
        preference_hits=30,
        preference_n=30,
    )
    assert gates["official_record_at_5"] is False
    assert gates["all_pass"] is False


def test_summarize_ks():
    rows = [
        _row("a", "t", hit_at_5=True, hit_at_8=True),
        _row("b", "t", hit_at_5=False, hit_at_8=True),
    ]
    s = summarize_ks(rows, ks=(5, 8))
    assert s["session_recall_at_5"] == 0.5
    assert s["hits_at_5"] == "1/2"
    assert s["session_recall_at_8"] == 1.0
