"""CLI for the synthetic shopping-delegation demo."""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

from .catalog import find_sku, price_on_day
from .dataset import DEFAULT_PATH, build_sessions, dumps_jsonl, write_jsonl
from .demo import DEMO_BRAIN, DEMO_LOG, Session, _chat, _delegation, _revocation, explain_saved, run_demo
from .policy import cue_for, decide, parse_record, render_record


def _selftest() -> None:
    from .policy import authoritative_record_id, days_between, safety_ok, structured_records_before

    assert days_between("2026-03-01T00:00:00Z", "2026-03-04T00:00:00Z") == 3.0
    rule = {
        "kind": "delegation",
        "record_id": "rule-1",
        "item": "whole_milk",
        "consent_id": "C1",
        "status": "active",
        "max_price_cents": "399",
        "brand_allow": "meadow_dairy,north_farm",
        "cadence_days": "3",
        "supersedes": "none",
        "ts": "2026-03-01T09:00:00Z",
        "source_uri": "consent://C1",
    }
    text = render_record(rule)
    assert parse_record(text)["consent_id"] == "C1"
    assert parse_record("CHAT hello") is None
    attempt = {
        "item": "whole_milk",
        "brand": "meadow_dairy",
        "price_cents": 349,
        "ts": "2026-03-02T12:00:00Z",
    }
    bought = decide([rule], attempt)
    assert bought["action"] == "buy" and bought["authorising_consent_id"] == "C1"
    pricey = dict(attempt, price_cents=500)
    assert decide([rule], pricey)["reason"] == "over_price"
    wrong = dict(attempt, brand="clear_filter")
    assert decide([rule], wrong)["reason"] == "brand"
    revoked = {
        "kind": "revocation",
        "record_id": "rev-1",
        "item": "whole_milk",
        "consent_id": "C1",
        "status": "revoked",
        "revokes": "rule-1",
        "ts": "2026-03-02T08:00:00Z",
        "source_uri": "consent://C1#revoked",
    }
    dead = decide([rule, revoked], attempt)
    assert dead["action"] == "refuse" and dead["reason"] == "revoked"
    assert dead["authorising_consent_id"] is None
    receipt = {
        "kind": "purchase",
        "record_id": "buy-1",
        "item": "whole_milk",
        "consent_id": "C1",
        "rule_id": "rule-1",
        "brand": "meadow_dairy",
        "price_cents": "349",
        "ts": "2026-03-02T12:00:00Z",
        "source_uri": "action://buy-1",
    }
    soon = dict(attempt, ts="2026-03-03T12:00:00Z")
    assert decide([rule, receipt], soon)["reason"] == "cadence"
    later = dict(attempt, ts="2026-03-06T12:00:00Z")
    assert decide([rule, receipt], later)["action"] == "buy"
    # A tighter later consent wins. The older looser cap must not.
    tighter = dict(rule, record_id="rule-2", consent_id="C2", max_price_cents="300", ts="2026-03-03T09:00:00Z", source_uri="consent://C2", supersedes="rule-1")
    mid = dict(attempt, price_cents=349, ts="2026-03-04T12:00:00Z")
    picked = decide([rule, tighter], mid)
    assert picked["reason"] == "over_price" and picked["authorising_consent_id"] == "C2"
    assert safety_ok("refuse", "buy") is True
    assert safety_ok("buy", "refuse") is False
    assert safety_ok("buy", "buy") is True
    events = [
        {"ts": rule["ts"], "text": text, "record": rule},
        {"ts": "2026-03-09T00:00:00Z", "role": "chat", "text": "CHAT rumor"},
    ]
    visible = structured_records_before(events, "2026-03-02T00:00:00Z")
    assert len(visible) == 1
    assert authoritative_record_id(visible, attempt) == "rule-1"

    first = dumps_jsonl(build_sessions())
    second = dumps_jsonl(build_sessions())
    assert first == second
    assert first.count('"label":"SYNTHETIC"') >= 37
    sessions = build_sessions()
    assert len(sessions) == 36
    reasons = {}
    for session in sessions:
        for attempt_row in session["attempts"]:
            reason = attempt_row["ground_truth"]["reason"]
            reasons[reason] = reasons.get(reason, 0) + 1
    for needed in ("in_policy", "over_price", "brand", "cadence", "revoked"):
        assert reasons.get(needed, 0) > 0, reasons
    print("selftest ok", json.dumps(reasons, sort_keys=True))


def _brain_session(path: Path, log: Path, fresh: bool) -> Session:
    session = Session(path, log)
    if fresh or not path.exists():
        session.open_new()
    else:
        session.memory = __import__(
            "shopping_delegation.memory", fromlist=["FluctlightMemory"]
        ).FluctlightMemory(str(path))
        if log.exists():
            for line in log.read_text(encoding="utf-8").splitlines():
                if line.strip():
                    session.actions.append(json.loads(line))
    return session


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Synthetic shopping delegation demo")
    sub = parser.add_subparsers(dest="cmd", required=True)

    sub.add_parser("demo", help="run the scripted session")
    sub.add_parser("selftest", help="policy and dataset checks, no native brain")

    gen = sub.add_parser("generate", help="write data/sessions.jsonl")
    gen.add_argument("--out", type=Path, default=DEFAULT_PATH)

    bench = sub.add_parser("bench", help="run memories on the synthetic set")
    bench.add_argument("--data", type=Path, default=DEFAULT_PATH)

    rec = sub.add_parser("record", help="store a standing delegation")
    rec.add_argument("--brain", type=Path, default=DEMO_BRAIN)
    rec.add_argument("--item", required=True)
    rec.add_argument("--brands", required=True, help="comma-separated allowlist")
    rec.add_argument("--max-price-cents", type=int, required=True)
    rec.add_argument("--cadence-days", type=int, default=3)
    rec.add_argument("--consent", required=True)
    rec.add_argument("--ts", default="2026-03-01T09:00:00Z")
    rec.add_argument("--record-id", default="")
    rec.add_argument("--supersedes", default="none")

    rev = sub.add_parser("revoke", help="revoke a consent")
    rev.add_argument("--brain", type=Path, default=DEMO_BRAIN)
    rev.add_argument("--item", required=True)
    rev.add_argument("--consent", required=True)
    rev.add_argument("--rule-id", required=True)
    rev.add_argument("--ts", default="2026-03-05T09:00:00Z")

    buy = sub.add_parser("buy", help="attempt one purchase from memory")
    buy.add_argument("--brain", type=Path, default=DEMO_BRAIN)
    buy.add_argument("--item", required=True)
    buy.add_argument("--brand", required=True)
    buy.add_argument("--sku", default="")
    buy.add_argument("--price-cents", type=int, default=None)
    buy.add_argument("--day", type=int, default=0, help="catalog day if price omitted and sku is known")
    buy.add_argument("--ts", default="2026-03-02T12:00:00Z")

    exp = sub.add_parser("explain", help="print an action chain from the demo log")
    exp.add_argument("action_id", nargs="?", default="last")
    exp.add_argument("--log", type=Path, default=DEMO_LOG)

    args = parser.parse_args(argv)
    if args.cmd == "selftest":
        _selftest()
        return 0
    if args.cmd == "generate":
        path = write_jsonl(args.out)
        print(path)
        return 0
    if args.cmd == "demo":
        run_demo()
        return 0
    if args.cmd == "bench":
        from .bench import run

        run(args.data)
        return 0
    if args.cmd == "explain":
        print(explain_saved(args.action_id, args.log))
        return 0

    log = DEMO_LOG if args.brain == DEMO_BRAIN else args.brain.parent / "actions.jsonl"
    if args.cmd == "record":
        session = _brain_session(args.brain, log, fresh=False)
        try:
            record_id = args.record_id or f"rule-{args.consent}"
            event = _delegation(
                record_id=record_id,
                item=args.item,
                consent_id=args.consent,
                brands=[b.strip() for b in args.brands.split(",") if b.strip()],
                cap=args.max_price_cents,
                cadence=args.cadence_days,
                when=args.ts,
                supersedes=args.supersedes,
            )
            report = session.write(event)
            print(json.dumps({"record_id": record_id, **report}, sort_keys=True))
        finally:
            session.close()
        return 0
    if args.cmd == "revoke":
        session = _brain_session(args.brain, log, fresh=False)
        try:
            event = _revocation(
                record_id=f"rev-{args.consent}",
                item=args.item,
                consent_id=args.consent,
                rule_id=args.rule_id,
                when=args.ts,
            )
            report = session.write(event)
            print(json.dumps(report, sort_keys=True))
        finally:
            session.close()
        return 0
    if args.cmd == "buy":
        price = args.price_cents
        sku = args.sku or f"{args.item}-{args.brand}"
        if price is None:
            product = find_sku(sku)
            price = price_on_day(product, args.day)
        session = _brain_session(args.brain, log, fresh=False)
        try:
            action = session.purchase(
                {
                    "attempt_id": sku,
                    "ts": args.ts,
                    "item": args.item,
                    "sku": sku,
                    "brand": args.brand,
                    "price_cents": price,
                }
            )
            print(json.dumps({"action_id": action["action_id"], "decision": action["decision"]}, sort_keys=True))
            print("cue:", cue_for(args.item))
        finally:
            session.close()
        return 0
    parser.error(args.cmd)
    return 2


if __name__ == "__main__":
    sys.exit(main())
