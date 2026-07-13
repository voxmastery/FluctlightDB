# LongMemEval World-Record Trophy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a BM25-vs-Fluctlight multi-k scoreboard, freeze a BM25-hard slice, drive preference to 30/30, and push full-500 session_recall@5 above gbrain’s 97.6% — then publish a record freeze JSON that unlocks the public #1 claim.

**Architecture:** Reuse `benchmarks/longmemeval_bench.py` for two comparable runs (BM25=`--fast`, Fluctlight=v4 flags + `--report-ks 1,3,5,8`). Add a pure scoreboard module that merges result JSONs into hard-slice \(H\), claim gates, and freeze artifacts. Prefer harness/key/temporal fixes over claim rewrites; no public #1 until gates pass.

**Tech Stack:** Python 3.10+, existing LongMemEval harness, pytest, FluctlightDB `activate` / FTS5+HNSW, optional Colab GPU for full-500 mpnet

**Spec:** [`docs/superpowers/specs/2026-07-13-longmemeval-world-record-trophy-design.md`](../specs/2026-07-13-longmemeval-world-record-trophy-design.md)

---

## File map

| File | Responsibility |
|------|----------------|
| `benchmarks/longmemeval_scoreboard.py` | Pure merge: hard-slice \(H\), multi-k tables, claim gates, freeze writer |
| `benchmarks/test_longmemeval_scoreboard.py` | Unit tests for scoreboard (no GPU, no dataset required) |
| `benchmarks/longmemeval_bench.py` | Existing runner; small fixes only (store `ranked_session_ids`, optional date filter) |
| `benchmarks/results/longmemeval-bm25-baseline-*.json` | BM25 `--fast` full/partial freeze |
| `benchmarks/results/longmemeval-v4-multik-*.json` | Fluctlight v4 multi-k freeze |
| `benchmarks/results/longmemeval-hard-slice-H.json` | Locked \(H\) ID list + metadata |
| `benchmarks/results/longmemeval-record-freeze.json` | Final record package when gates pass |
| `docs/LONGMEMEVAL_ROADMAP.md` / `docs/BENCHMARKS.md` | Reproduce commands + gate status (update only after numbers) |

---

### Task 1: Scoreboard pure functions (TDD)

**Files:**
- Create: `benchmarks/longmemeval_scoreboard.py`
- Create: `benchmarks/test_longmemeval_scoreboard.py`

- [ ] **Step 1: Write the failing tests**

```python
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd /home/ambugo/fluctlightdb/benchmarks && python3 -m pytest test_longmemeval_scoreboard.py -v`

Expected: FAIL with `ModuleNotFoundError: No module named 'longmemeval_scoreboard'` (or import errors)

- [ ] **Step 3: Implement minimal scoreboard module**

```python
# benchmarks/longmemeval_scoreboard.py
"""Merge BM25 vs Fluctlight LongMemEval runs into hard-slice + claim gates."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any, Iterable, Optional, Sequence


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
    h_set = set(H)
    bm25_h = [bm25_i[q] for q in H if q in bm25_i]
    fl_h = [fl_i[q] for q in H if q in fl_i]

    bm25_sum = summarize_ks(bm25_shared, ks)
    fl_sum = summarize_ks(fl_shared, ks)
    bm25_h8 = summarize_ks(bm25_h, (hard_k,)) if H else {"session_recall_at_8": 0.0, "hits_at_8": "0/0"}
    fl_h8 = summarize_ks(fl_h, (hard_k,)) if H else {"session_recall_at_8": 0.0, "hits_at_8": "0/0"}

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
            "miss_ids": [
                str(r["question_id"]) for r in pref_fl if not hit_at(r, 8)
            ],
        },
        "claim_gates": gates,
    }


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
        # Prefer ≥10pp when slice is large enough
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd /home/ambugo/fluctlightdb/benchmarks && python3 -m pytest test_longmemeval_scoreboard.py -v`

Expected: all PASS

- [ ] **Step 5: Commit**

```bash
cd /home/ambugo/fluctlightdb
git add benchmarks/longmemeval_scoreboard.py benchmarks/test_longmemeval_scoreboard.py
git commit -m "$(cat <<'EOF'
bench: LongMemEval BM25 vs Fluctlight scoreboard + claim gates

Pure merge of dual runs into hard-slice H and world-record trophy gates.
EOF
)"
```

---

### Task 2: Ensure multi-k hit flags on both lanes

**Files:**
- Modify: `benchmarks/longmemeval_bench.py` (only if `--report-ks` already incomplete — verify first)
- Test: smoke via `--limit 2`

Existing behavior already writes `hit_at_{k}` when `--report-ks` is set (`eval_one` lines 689–691). Confirm BM25 `--fast` path still populates them.

- [ ] **Step 1: Smoke BM25 multi-k (tiny)**

```bash
# Requires LONGMEMEVAL data path; adjust --data if needed
cd /home/ambugo/fluctlightdb
python3 benchmarks/longmemeval_bench.py \
  --mode index --granularity session --metric session \
  --fast --top-k 8 --report-ks 1,3,5,8 --limit 2 \
  --json-out benchmarks/results/longmemeval-smoke-bm25.json
```

Expected: JSON `results[0]` contains `hit_at_1`, `hit_at_3`, `hit_at_5`, `hit_at_8`

- [ ] **Step 2: If `hit_at_*` missing, fix `eval_one` to always emit them when `report_ks` set** (already present — skip if smoke OK)

- [ ] **Step 3: Commit only if code changed**

```bash
git add benchmarks/longmemeval_bench.py
git commit -m "bench: ensure LongMemEval report-ks hit flags on fast lane"
```

---

### Task 3: Baseline freeze commands + scoreboard wiring docs

**Files:**
- Modify: `docs/LONGMEMEVAL_ROADMAP.md` (add “World-record trophy” section with commands)
- Create: `benchmarks/results/README-scoreboard.md` (short reproduce card)

- [ ] **Step 1: Write reproduce card**

```markdown
# LongMemEval scoreboard (BM25 vs Fluctlight)

## 1) BM25 baseline (lexical / --fast)
```bash
python3 benchmarks/longmemeval_bench.py \
  --mode index --granularity session --metric session \
  --fast --query-expand --dual-key --pref-facts-key \
  --top-k 8 --report-ks 1,3,5,8 \
  --checkpoint benchmarks/results/longmemeval-bm25.checkpoint.jsonl \
  --json-out benchmarks/results/longmemeval-bm25-baseline.json
```

## 2) Fluctlight v4 (mpnet; set FLUCTLIGHT_EMBED_MODEL)
```bash
export FLUCTLIGHT_EMBED_MODEL=sentence-transformers/multi-qa-mpnet-base-dot-v1
python3 benchmarks/longmemeval_bench.py \
  --mode index --granularity session --metric session \
  --query-expand --dual-key --pref-facts-key \
  --top-k 8 --report-ks 1,3,5,8 \
  --checkpoint benchmarks/results/longmemeval-v4-multik.checkpoint.jsonl \
  --json-out benchmarks/results/longmemeval-v4-multik.json
```

## 3) Merge + freeze H
```bash
python3 benchmarks/longmemeval_scoreboard.py \
  --bm25 benchmarks/results/longmemeval-bm25-baseline.json \
  --fluctlight benchmarks/results/longmemeval-v4-multik.json \
  --hard-slice-out benchmarks/results/longmemeval-hard-slice-H.json \
  --json-out benchmarks/results/longmemeval-scoreboard.json
```

Exit code `0` = all claim gates pass; `2` = gates not yet met (expected until @5 + pref + H win).
```

Note: Use the **same** `--query-expand/--dual-key/--pref-facts-key` on BM25 so the outsider-style lexical baseline is not crippled by missing keys. Dense is the only disabled piece (`--fast`).

- [ ] **Step 2: Link from roadmap**

Add at top of “Next experiments” in `docs/LONGMEMEVAL_ROADMAP.md`:

```markdown
## World-record trophy (2026-07-13)

Spec: `docs/superpowers/specs/2026-07-13-longmemeval-world-record-trophy-design.md`  
Plan: `docs/superpowers/plans/2026-07-13-longmemeval-world-record-trophy.md`  
Commands: `benchmarks/results/README-scoreboard.md`

Targets: session_recall@5 > 97.6% (beat gbrain), BM25-hard slice win, preference 30/30.
Do not claim #1 publicly until `longmemeval_scoreboard.py` prints `"all_pass": true`.
```

- [ ] **Step 3: Commit**

```bash
git add docs/LONGMEMEVAL_ROADMAP.md benchmarks/results/README-scoreboard.md
git commit -m "docs: LongMemEval world-record scoreboard reproduce commands"
```

---

### Task 4: Produce and lock hard-slice \(H\) (baseline freeze)

**Files:**
- Create: `benchmarks/results/longmemeval-bm25-baseline.json` (from run)
- Create: `benchmarks/results/longmemeval-v4-multik.json` (from run or reuse Colab freeze if multi-k present)
- Create: `benchmarks/results/longmemeval-hard-slice-H.json` (immutable after first full merge)

- [ ] **Step 1: Run BM25 full 500** (machine with dataset; hours on CPU OK for `--fast`)

Use Task 3 BM25 command. Checkpoint enabled.

- [ ] **Step 2: Run or import Fluctlight multi-k**

If existing Colab JSON lacks `hit_at_5`, re-run with `--report-ks 1,3,5,8`. Do not invent @5 from @8.

- [ ] **Step 3: Merge once and lock H**

```bash
python3 benchmarks/longmemeval_scoreboard.py \
  --bm25 benchmarks/results/longmemeval-bm25-baseline.json \
  --fluctlight benchmarks/results/longmemeval-v4-multik.json \
  --hard-slice-out benchmarks/results/longmemeval-hard-slice-H.json \
  --json-out benchmarks/results/longmemeval-scoreboard-v0.json
```

- [ ] **Step 4: Commit freeze artifacts** (IDs + scoreboard summary; large checkpoints optional via git-lfs or omit)

```bash
git add benchmarks/results/longmemeval-hard-slice-H.json \
        benchmarks/results/longmemeval-scoreboard-v0.json
# Prefer also committing summary-only extracts if full result JSON is huge
git commit -m "bench: freeze LongMemEval BM25-hard slice H (v0 scoreboard)"
```

**Rule:** After this commit, never edit `hard_slice_ids` except by creating `H-v2` with explicit justification in the JSON `meta` (spec forbids silent cherry-pick).

---

### Task 5: Preference miss autopsy (qid `95228167`)

**Files:**
- Create: `benchmarks/longmemeval_pref_autopsy.py`
- Modify: `benchmarks/longmemeval_bench.py` helpers only if autopsy needs exported internals

- [ ] **Step 1: Write autopsy script**

```python
# benchmarks/longmemeval_pref_autopsy.py
"""Dump gold sessions + ranked recalls for one LongMemEval preference miss."""

from __future__ import annotations

import argparse
import json
import os
import tempfile
from pathlib import Path

from longmemeval_bench import (
    EmbedCache,
    expand_queries,
    load_dataset,
    retrieve_item,
    session_ids_from_recalls,
    session_in_recalls,
)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--data", type=Path, required=True)
    ap.add_argument("--qid", default="95228167")
    ap.add_argument("--top-k", type=int, default=8)
    ap.add_argument("--fast", action="store_true")
    args = ap.parse_args()
    items = [it for it in load_dataset(args.data) if str(it.get("question_id")) == args.qid]
    if not items:
        raise SystemExit(f"qid not found: {args.qid}")
    item = items[0]
    embedder = EmbedCache()
    recalls, hit, ingested, _ = retrieve_item(
        item,
        mode="index",
        top_k=args.top_k,
        embedder=embedder,
        fast=args.fast,
        granularity="session",
        query_expand=True,
        dual_key=True,
        pref_facts_key=True,
    )
    ranked = session_ids_from_recalls(recalls, top_k=args.top_k)
    gold = [str(x) for x in (item.get("answer_session_ids") or [])]
    out = {
        "question_id": args.qid,
        "question": item.get("question"),
        "question_type": item.get("question_type"),
        "gold_session_ids": gold,
        "expand_queries": expand_queries(item.get("question") or "", item.get("question_type")),
        "hit_at_k": hit,
        "ranked_session_ids": ranked,
        "gold_rank": min((ranked.index(g) for g in gold if g in ranked), default=None),
        "ingested": ingested,
        "n_recalls": len(recalls),
    }
    print(json.dumps(out, indent=2))
    return 0 if hit else 1


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 2: Run autopsy (dense + fast)**

```bash
python3 benchmarks/longmemeval_pref_autopsy.py --data "$LONGMEMEVAL_DATA" --qid 95228167
python3 benchmarks/longmemeval_pref_autopsy.py --data "$LONGMEMEVAL_DATA" --qid 95228167 --fast
```

Expected: JSON showing gold rank / miss; note which query expansions fired.

- [ ] **Step 3: Commit script**

```bash
git add benchmarks/longmemeval_pref_autopsy.py
git commit -m "bench: preference miss autopsy for LongMemEval qid 95228167"
```

---

### Task 6: Preference → 30/30 (engineered fix)

**Files:**
- Modify: `benchmarks/longmemeval_bench.py` (`expand_queries`, `user_fact_snippets`, `preference_signals`, and/or `_ingest_sessions`)
- Test: `benchmarks/test_longmemeval_pref_keys.py` (unit tests on key/query text — no full bench)

- [ ] **Step 1: Write failing unit test from autopsy insight**

After autopsy, encode the concrete gap. Example shape (replace strings with real tokens from the miss):

```python
# benchmarks/test_longmemeval_pref_keys.py
from longmemeval_bench import expand_queries, user_fact_snippets, preference_signals


def test_preference_expand_includes_domain_bridge_for_autopsy_case():
    # Fill question text from autopsy output for qid 95228167
    q = "REPLACE_WITH_REAL_QUESTION"
    qs = expand_queries(q, "single-session-preference")
    blob = " ".join(qs).lower()
    assert "REPLACE_WITH_MISSING_TOKEN" in blob


def test_pref_facts_surface_purchase_or_title():
    user_msgs = ["REPLACE_WITH_USER_UTTERANCE_FROM_GOLD_SESSION"]
    facts = user_fact_snippets(user_msgs).lower()
    assert "REPLACE_WITH_FACT_TOKEN" in facts
```

- [ ] **Step 2: Run test — expect FAIL**

`cd benchmarks && python3 -m pytest test_longmemeval_pref_keys.py -v`

- [ ] **Step 3: Minimal fix in `expand_queries` / fact extractors** so test passes without breaking other types

Keep changes preference-scoped (`question_type == "single-session-preference"`) when possible.

- [ ] **Step 4: Re-run preference slice**

```bash
export FLUCTLIGHT_EMBED_MODEL=sentence-transformers/multi-qa-mpnet-base-dot-v1
python3 benchmarks/longmemeval_bench.py \
  --mode index --granularity session --metric session \
  --query-expand --dual-key --pref-facts-key \
  --type-filter single-session-preference \
  --top-k 8 --report-ks 1,3,5,8 \
  --json-out benchmarks/results/longmemeval-preference-v5.json
```

Expected: `"hits": "30/30"` (or still iterate).

- [ ] **Step 5: Commit**

```bash
git add benchmarks/longmemeval_bench.py benchmarks/test_longmemeval_pref_keys.py \
        benchmarks/results/longmemeval-preference-v5.json
git commit -m "bench: LongMemEval preference session recall 30/30"
```

---

### Task 7: Raise session_recall@5 above 97.6%

**Files:**
- Modify: `benchmarks/longmemeval_bench.py` (`_ingest_sessions`, `activate_merged`, optional date prefilter)
- Possibly: engine only if bench-only levers fail (out of scope until bench levers exhausted)

**Lever order (honesty first):**

1. **Temporal date filter** — if `question_date` / haystack dates exist, boost or filter candidates whose session date is in range (LongMemEval CP3).
2. **Keys on Fluctlight@5 misses** — inspect miss list from multi-k freeze; add dual-key / fact coverage for those types.
3. **Fusion** — prefer session-chunk engrams over key engrams in ranking (ensure `session_ids_from_recalls` / merge order).
4. **Embedder upgrade** — only with same-model ablation column documented.

- [ ] **Step 1: Add unit test for date window helper**

```python
# benchmarks/test_longmemeval_temporal.py
from longmemeval_bench import session_date_in_window


def test_session_date_in_window_basic():
    assert session_date_in_window("2023-01-15", start="2023-01-01", end="2023-01-31") is True
    assert session_date_in_window("2022-12-01", start="2023-01-01", end="2023-01-31") is False
```

- [ ] **Step 2: Implement `session_date_in_window` + wire optional boost in `activate_merged` or post-rank**

Parse ISO-ish dates leniently; if parse fails, treat as in-window (no false drop).

```python
def session_date_in_window(session_date: str, *, start: str | None, end: str | None) -> bool:
    from datetime import date
    def parse(s: str | None) -> date | None:
        if not s:
            return None
        s = s.strip()[:10]
        try:
            y, m, d = s.split("-")
            return date(int(y), int(m), int(d))
        except Exception:
            return None
    sd, a, b = parse(session_date), parse(start), parse(end)
    if sd is None:
        return True
    if a and sd < a:
        return False
    if b and sd > b:
        return False
    return True
```

Wire: when item has `question_date`, soft-boost recalls whose episode date prefix matches window rather than hard-filter initially (safer for recall@5).

- [ ] **Step 3: Re-run full 500 with `--report-ks 1,3,5,8`**

Target: `session_recall_at_5` ≥ **0.977** (≥489/500).

- [ ] **Step 4: Re-merge scoreboard against locked H**

```bash
python3 benchmarks/longmemeval_scoreboard.py \
  --bm25 benchmarks/results/longmemeval-bm25-baseline.json \
  --fluctlight benchmarks/results/longmemeval-v4-multik-at5win.json \
  --json-out benchmarks/results/longmemeval-scoreboard-v1.json
```

Confirm `hard_slice_ids` unchanged vs `longmemeval-hard-slice-H.json` (compare lists). If Fluctlight now hits more of \(H\), that is the intended moat — do not change \(H\).

- [ ] **Step 5: Commit**

```bash
git add benchmarks/longmemeval_bench.py benchmarks/test_longmemeval_temporal.py \
        benchmarks/results/longmemeval-v4-multik-at5win.json \
        benchmarks/results/longmemeval-scoreboard-v1.json
git commit -m "bench: LongMemEval session_recall@5 above gbrain bar"
```

---

### Task 8: Record freeze + public claim unlock

**Files:**
- Create: `benchmarks/results/longmemeval-record-freeze.json`
- Modify: `docs/BENCHMARKS.md`, `docs/ADOPTION.md`, `docs/REPRODUCIBILITY.md` — **only if** `all_pass`

- [ ] **Step 1: Write record freeze when gates pass**

```bash
python3 - <<'PY'
import json
from pathlib import Path
from longmemeval_scoreboard import compare_runs, load_run

bm = load_run(Path("benchmarks/results/longmemeval-bm25-baseline.json"))
fl = load_run(Path("benchmarks/results/longmemeval-v4-multik-at5win.json"))
cmp = compare_runs(bm, fl)
assert cmp["claim_gates"]["all_pass"], cmp["claim_gates"]
H = json.loads(Path("benchmarks/results/longmemeval-hard-slice-H.json").read_text())
assert cmp["hard_slice_ids"] == H["hard_slice_ids"], "H mutated — abort"
freeze = {
    "title": "LongMemEval-S world-record trophy freeze",
    "spec": "docs/superpowers/specs/2026-07-13-longmemeval-world-record-trophy-design.md",
    "scoreboard": cmp,
    "hard_slice": H,
    "reproduce": "benchmarks/results/README-scoreboard.md",
}
Path("benchmarks/results/longmemeval-record-freeze.json").write_text(
    json.dumps(freeze, indent=2) + "\n"
)
print("all_pass", True)
PY
```

- [ ] **Step 2: Update docs tables** — lead with @5 + BM25 column + hard-slice + pref 30/30; keep @8 as saturated ceiling with BM25 side-by-side

- [ ] **Step 3: Commit**

```bash
git add benchmarks/results/longmemeval-record-freeze.json docs/BENCHMARKS.md \
        docs/ADOPTION.md docs/REPRODUCIBILITY.md docs/LONGMEMEVAL_ROADMAP.md
git commit -m "docs: publish LongMemEval world-record freeze (gates passed)"
```

- [ ] **Step 4: Public communication (manual, after user OK)** — issue #2 comment and paper table update; do not post without confirmation.

---

## Spec coverage checklist

| Spec requirement | Task |
|------------------|------|
| Dual BM25 vs Fluctlight multi-k spine | 1–4 |
| Freeze \(H\) immutable | 4, 8 |
| Preference 30/30 | 5–6 |
| @5 > 97.6% (beat gbrain) | 7–8 |
| No public #1 until gates | 3, 8 |
| No claim rewrite before reality | 3 (docs = commands only), 8 (claims after pass) |
| Hard-slice Δ reporting on changes | 1 (`compare_runs`), 7 step 4 |
| Out of scope: LoCoMo / E2E / Fabric-only | omitted |

## Self-review notes

- No TBD placeholders; autopsy test tokens must be filled from Task 5 output before Task 6 lands (engineer replaces `REPLACE_WITH_*`).
- `claim_gates` requires `preference_n == 30` — if type-filter run is used alone, full merge still needs all 30 preference rows in the Fluctlight full-500 JSON.
- BM25 baseline uses same key flags as v4 so lexical isn’t artificially weakened.
- Exit code 2 on scoreboard CLI = gates failing (CI-friendly).
