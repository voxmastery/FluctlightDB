"""Deterministic synthetic shopping sessions.

Every row is labeled synthetic. Shoppers, consents, and prices are invented.
Regenerate with ``python -m shopping_delegation generate`` (seed 20260330).
"""

from __future__ import annotations

import json
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

from .policy import (
    CUE_MARKERS,
    DATASET_NAME,
    SEED,
    authoritative_record_id,
    decide,
    render_record,
    structured_records_before,
)

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_PATH = ROOT / "data" / "sessions.jsonl"

BASE = datetime(2026, 3, 1, tzinfo=timezone.utc)

ITEMS: list[dict[str, Any]] = [
    {
        "slug": "whole_milk",
        "brands": ["meadow_dairy", "north_farm", "clear_filter", "house_brand"],
        "base": 349,
    },
    {
        "slug": "oat_milk",
        "brands": ["grain_cup", "small_batch", "west_oat", "house_brand"],
        "base": 429,
    },
    {
        "slug": "eggs",
        "brands": ["pasture_coop", "hen_yard", "morning_dozen", "house_brand"],
        "base": 549,
    },
    {
        "slug": "bread",
        "brands": ["hearth_loaf", "gold_crust", "soft_slice", "house_brand"],
        "base": 399,
    },
    {
        "slug": "coffee",
        "brands": ["harbor_roast", "dark_kiln", "ridge_brew", "house_brand"],
        "base": 899,
    },
    {
        "slug": "yogurt",
        "brands": ["cup_culture", "thick_pot", "alpine_cup", "house_brand"],
        "base": 179,
    },
]


def ts(day: int, hour: int = 12, minute: int = 0) -> str:
    stamp = BASE + timedelta(days=day, hours=hour, minutes=minute)
    return stamp.strftime("%Y-%m-%dT%H:%M:%SZ")


def _markers_prefix(item: str) -> str:
    return " ".join(CUE_MARKERS) + f" item {item}"


def _delegation(
    *,
    session_id: str,
    item: str,
    version: int,
    consent_id: str,
    brands: list[str],
    cap: int,
    cadence: int,
    when: str,
    supersedes: str,
) -> dict[str, Any]:
    record_id = f"rule-{session_id}-{item}-v{version}"
    fields = {
        "kind": "delegation",
        "record_id": record_id,
        "item": item,
        "consent_id": consent_id,
        "status": "active",
        "max_price_cents": cap,
        "brand_allow": ",".join(brands),
        "cadence_days": cadence,
        "supersedes": supersedes,
        "ts": when,
        "source_uri": f"consent://{consent_id}",
    }
    text = render_record(fields) + " " + _markers_prefix(item)
    return {"ts": when, "role": "delegation", "text": text, "record": {k: str(v) for k, v in fields.items()}}


def _revocation(*, session_id: str, item: str, consent_id: str, rule_id: str, when: str) -> dict[str, Any]:
    record_id = f"rev-{session_id}-{item}"
    fields = {
        "kind": "revocation",
        "record_id": record_id,
        "item": item,
        "consent_id": consent_id,
        "status": "revoked",
        "revokes": rule_id,
        "ts": when,
        "source_uri": f"consent://{consent_id}#revoked",
    }
    text = render_record(fields) + " " + _markers_prefix(item)
    return {"ts": when, "role": "revocation", "text": text, "record": {k: str(v) for k, v in fields.items()}}


def _purchase(
    *,
    session_id: str,
    item: str,
    consent_id: str,
    rule_id: str,
    brand: str,
    price: int,
    when: str,
) -> dict[str, Any]:
    record_id = f"buy-{session_id}-{item}"
    fields = {
        "kind": "purchase",
        "record_id": record_id,
        "item": item,
        "consent_id": consent_id,
        "rule_id": rule_id,
        "sku": f"{item}-{brand}",
        "brand": brand,
        "price_cents": price,
        "status": "settled",
        "ts": when,
        "source_uri": f"action://{record_id}",
    }
    text = render_record(fields) + " " + _markers_prefix(item)
    return {"ts": when, "role": "purchase", "text": text, "record": {k: str(v) for k, v in fields.items()}}


def _chat(item: str, when: str, wrong_price: int, bad_brand: str, nonce: str) -> dict[str, Any]:
    # Short, cue-heavy, and not a RECORD. Unique nonce tokens keep the
    # separation gate from collapsing the distractors into one engram.
    text = (
        f"CHAT kind=chat item={item} ts={when} "
        + _markers_prefix(item)
        + f" rumor max_price_cents={wrong_price} brand_allow={bad_brand} {nonce}"
    )
    return {"ts": when, "role": "chat", "text": text}


def _nonce(session_index: int, chat_index: int) -> str:
    return " ".join(f"n{session_index:02d}x{chat_index:02d}t{k:02d}" for k in range(8))


def _attempt(
    session_id: str,
    n: int,
    when: str,
    item: str,
    brand: str,
    price: int,
    events: list[dict[str, Any]],
) -> dict[str, Any]:
    attempt = {
        "attempt_id": f"{session_id}-A{n}",
        "ts": when,
        "item": item,
        "sku": f"{item}-{brand}",
        "brand": brand,
        "price_cents": price,
    }
    visible = structured_records_before(events, when)
    ground = decide(visible, attempt)
    ground["authoritative_record_id"] = authoritative_record_id(visible, attempt)
    attempt["ground_truth"] = ground
    return attempt


def build_session(index: int) -> dict[str, Any]:
    if index < 6:
        cohort = "A_cadence"
    elif index < 12:
        cohort = "A_single"
    elif index < 24:
        cohort = "B_update"
    elif index < 30:
        cohort = "C_stack_revoke"
    else:
        cohort = "D_revoke"

    focus = ITEMS[index % len(ITEMS)]
    item = str(focus["slug"])
    base = int(focus["base"])
    allowed = list(focus["brands"][:2])
    blocked = str(focus["brands"][2])
    session_id = f"S{index:02d}"
    cap1 = base + 80
    cap2 = base + 30
    cap3 = base + 10
    cadence = 3

    events: list[dict[str, Any]] = []
    for offset in (1, 2, 3):
        other = ITEMS[(index + offset) % len(ITEMS)]
        oslug = str(other["slug"])
        events.append(
            _delegation(
                session_id=session_id,
                item=oslug,
                version=1,
                consent_id=f"consent-{session_id}-{oslug}-v1",
                brands=list(other["brands"][:2]),
                cap=int(other["base"]) + 80,
                cadence=cadence,
                when=ts(0, 8, offset),
                supersedes="none",
            )
        )

    consent1 = f"consent-{session_id}-{item}-v1"
    events.append(
        _delegation(
            session_id=session_id,
            item=item,
            version=1,
            consent_id=consent1,
            brands=allowed,
            cap=cap1,
            cadence=cadence,
            when=ts(0, 9),
            supersedes="none",
        )
    )
    rule1 = f"rule-{session_id}-{item}-v1"

    for c in range(8):
        events.append(
            _chat(
                item,
                ts(1, 9, c),
                wrong_price=cap1 + 400 + c,
                bad_brand=blocked,
                nonce=_nonce(index, c),
            )
        )
    # Stale restatement of the original cap, still just chat.
    events.append(
        _chat(
            item,
            ts(2, 8),
            wrong_price=cap1,
            bad_brand=allowed[0],
            nonce=_nonce(index, 20),
        )
    )

    active_consent = consent1
    active_rule = rule1
    if cohort in {"B_update", "C_stack_revoke"}:
        consent2 = f"consent-{session_id}-{item}-v2"
        events.append(
            _delegation(
                session_id=session_id,
                item=item,
                version=2,
                consent_id=consent2,
                brands=allowed,
                cap=cap2,
                cadence=cadence,
                when=ts(2, 9),
                supersedes=rule1,
            )
        )
        active_consent = consent2
        active_rule = f"rule-{session_id}-{item}-v2"
    if cohort == "C_stack_revoke":
        consent3 = f"consent-{session_id}-{item}-v3"
        events.append(
            _delegation(
                session_id=session_id,
                item=item,
                version=3,
                consent_id=consent3,
                brands=allowed[:1],
                cap=cap3,
                cadence=cadence,
                when=ts(3, 9),
                supersedes=active_rule,
            )
        )
        active_consent = consent3
        active_rule = f"rule-{session_id}-{item}-v3"

    if cohort == "A_cadence":
        events.append(
            _purchase(
                session_id=session_id,
                item=item,
                consent_id=active_consent,
                rule_id=active_rule,
                brand=allowed[0],
                price=base,
                when=ts(3, 12),
            )
        )

    attempts: list[dict[str, Any]] = []

    def add(day: int, hour: int, brand: str, price: int) -> None:
        attempts.append(
            _attempt(session_id, len(attempts) + 1, ts(day, hour), item, brand, price, events)
        )

    if cohort == "A_cadence":
        add(4, 12, allowed[0], base)  # inside cadence window
        add(8, 12, allowed[0], base)  # cadence elapsed
    else:
        add(4, 10, allowed[0], base)  # in policy before any revocation

    if cohort in {"B_update", "C_stack_revoke"}:
        add(4, 11, allowed[0], base + 50)  # under v1 cap, over v2/v3 cap
    add(4, 13, allowed[0], cap1 + 200)  # over every cap
    add(4, 14, blocked, base)  # brand not allowlisted
    if cohort == "C_stack_revoke":
        add(4, 15, allowed[1], base)  # v3 dropped the second brand

    if cohort in {"C_stack_revoke", "D_revoke"}:
        events.append(
            _revocation(
                session_id=session_id,
                item=item,
                consent_id=active_consent,
                rule_id=active_rule,
                when=ts(5, 9),
            )
        )
        add(6, 12, allowed[0], base)

    events_sorted = sorted(events, key=lambda e: (e["ts"], e["text"]))
    # Ground truth was computed against the unsorted event list; timestamps
    # decide visibility, so order does not change it. Recompute to be sure.
    visible_check_events = events_sorted
    for attempt in attempts:
        visible = structured_records_before(visible_check_events, attempt["ts"])
        fresh = decide(visible, attempt)
        fresh["authoritative_record_id"] = authoritative_record_id(visible, attempt)
        if fresh != attempt["ground_truth"]:
            raise RuntimeError(f"ground truth drift on {attempt['attempt_id']}")

    return {
        "synthetic": True,
        "dataset": DATASET_NAME,
        "seed": SEED,
        "session_id": session_id,
        "cohort": cohort,
        "focus_item": item,
        "label": "SYNTHETIC",
        "events": events_sorted,
        "attempts": attempts,
    }


def build_sessions() -> list[dict[str, Any]]:
    return [build_session(i) for i in range(36)]


def header() -> dict[str, Any]:
    return {
        "synthetic": True,
        "label": "SYNTHETIC",
        "record_type": "dataset_header",
        "dataset": DATASET_NAME,
        "seed": SEED,
        "n_sessions": 36,
        "note": (
            "Invented shoppers, consents, brands, and prices. "
            "Not logs from any store or person. Do not generalise."
        ),
    }


def dumps_jsonl(sessions: list[dict[str, Any]]) -> str:
    lines = [json.dumps(header(), sort_keys=True, separators=(",", ":"))]
    for session in sessions:
        lines.append(json.dumps(session, sort_keys=True, separators=(",", ":")))
    return "\n".join(lines) + "\n"


def write_jsonl(path: Path = DEFAULT_PATH) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(dumps_jsonl(build_sessions()), encoding="utf-8")
    return path


def load_sessions(path: Path = DEFAULT_PATH) -> list[dict[str, Any]]:
    sessions: list[dict[str, Any]] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        row = json.loads(line)
        if row.get("record_type") == "dataset_header":
            if row.get("label") != "SYNTHETIC" or row.get("synthetic") is not True:
                raise ValueError("dataset header is not labeled SYNTHETIC")
            continue
        if row.get("synthetic") is not True or row.get("label") != "SYNTHETIC":
            raise ValueError(f"session {row.get('session_id')} missing SYNTHETIC label")
        sessions.append(row)
    return sessions
