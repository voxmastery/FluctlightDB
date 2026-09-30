"""Scripted demo: record a delegation, try purchases, update, revoke, explain."""

from __future__ import annotations

import json
import shutil
from pathlib import Path
from typing import Any, Optional

from .catalog import describe_catalog
from .memory import FluctlightMemory
from .policy import cue_for, decide, parse_record
from .viz import write_session_page

ROOT = Path(__file__).resolve().parents[1]
VAR = ROOT / "var"
DEMO_BRAIN = VAR / "demo-brain"
DEMO_LOG = VAR / "demo-actions.jsonl"


def _delegation(
    *,
    record_id: str,
    item: str,
    consent_id: str,
    brands: list[str],
    cap: int,
    cadence: int,
    when: str,
    supersedes: str,
) -> dict[str, Any]:
    record = {
        "kind": "delegation",
        "record_id": record_id,
        "item": item,
        "consent_id": consent_id,
        "status": "active",
        "max_price_cents": str(cap),
        "brand_allow": ",".join(brands),
        "cadence_days": str(cadence),
        "supersedes": supersedes,
        "ts": when,
        "source_uri": f"consent://{consent_id}",
    }
    from .policy import CUE_MARKERS, render_record

    text = render_record(record) + " " + " ".join(CUE_MARKERS) + f" item {item}"
    return {"role": "delegation", "text": text, "record": record, "ts": when}


def _revocation(*, record_id: str, item: str, consent_id: str, rule_id: str, when: str) -> dict[str, Any]:
    from .policy import CUE_MARKERS, render_record

    record = {
        "kind": "revocation",
        "record_id": record_id,
        "item": item,
        "consent_id": consent_id,
        "status": "revoked",
        "revokes": rule_id,
        "ts": when,
        "source_uri": f"consent://{consent_id}#revoked",
    }
    text = render_record(record) + " " + " ".join(CUE_MARKERS) + f" item {item}"
    return {"role": "revocation", "text": text, "record": record, "ts": when}


def _chat(text: str, when: str) -> dict[str, Any]:
    return {"role": "chat", "text": text, "record": {}, "ts": when}


def wipe_brain(path: Path) -> None:
    """Remove a brain directory and the sibling lock, WAL, and index files.

    ``connect_embedded`` keeps ``<path>.flct.wal.*``, ``<path>.flct.index.sqlite``,
    and ``<path>.lock`` next to the directory. Deleting only the directory replays
    the old WAL on the next open.
    """
    parent = path.parent
    name = path.name
    if not parent.exists():
        return
    for child in parent.iterdir():
        if child.name != name and not child.name.startswith(name + "."):
            continue
        if child.is_dir():
            shutil.rmtree(child)
        else:
            child.unlink()


class Session:
    def __init__(self, brain_path: Path, log_path: Path) -> None:
        self.brain_path = brain_path
        self.log_path = log_path
        self.memory: Optional[FluctlightMemory] = None
        self.known: list[dict[str, str]] = []
        self.actions: list[dict[str, Any]] = []
        self.seq = 0

    def open_new(self) -> None:
        wipe_brain(self.brain_path)
        self.brain_path.parent.mkdir(parents=True, exist_ok=True)
        self.memory = FluctlightMemory(str(self.brain_path))
        self.known = []
        self.actions = []
        self.seq = 0
        if self.log_path.exists():
            self.log_path.unlink()

    def reopen(self) -> None:
        if self.memory is not None:
            self.memory.checkpoint()
            self.memory.close()
        self.memory = FluctlightMemory(str(self.brain_path))

    def close(self) -> None:
        if self.memory is not None:
            self.memory.checkpoint()
            self.memory.close()
            self.memory = None

    def _mem(self) -> FluctlightMemory:
        if self.memory is None:
            raise RuntimeError("brain is not open")
        return self.memory

    def write(self, event: dict[str, Any]) -> dict[str, Any]:
        from .memory import meta_for_event

        self.seq += 1
        report = self._mem().add(
            event["text"],
            meta=meta_for_event(event, session_id="demo", seq=self.seq),
        )
        parsed = parse_record(event["text"])
        if parsed is not None:
            self.known.append(parsed)
        return report

    def purchase(self, attempt: dict[str, Any]) -> dict[str, Any]:
        hits, elapsed = self._mem().recall(cue_for(str(attempt["item"])))
        recalled = []
        for hit in hits:
            parsed = parse_record(hit.text)
            if parsed is not None:
                recalled.append(parsed)
        decision = decide(recalled, attempt)
        cited = None
        for hit in hits:
            parsed = parse_record(hit.text)
            if parsed and parsed.get("record_id") == decision.get("cited_record_id"):
                cited = hit
                break
        action = {
            "action_id": f"A{len(self.actions) + 1:04d}",
            "attempt": attempt,
            "decision": decision,
            "latency_ms": round(elapsed, 4),
            "hits": [hit.as_dict() for hit in hits],
            "cited_hit": None if cited is None else cited.as_dict(),
        }
        self.actions.append(action)
        self.log_path.parent.mkdir(parents=True, exist_ok=True)
        with self.log_path.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(action, sort_keys=True) + "\n")
        return action

    def explain(self, action_id: str) -> str:
        action = next(a for a in self.actions if a["action_id"] == action_id)
        return format_explanation(action)

    def resolve_note(self, item: str) -> str:
        raw = self._mem().brain.resolve(cue_for(item))
        if not isinstance(raw, dict):
            return f"resolve() returned {raw!r}"
        value = str(raw.get("value") or "")
        return (
            "resolve() single winner "
            f"engram={raw.get('winner_engram_id')} contested={raw.get('contested')} "
            f"trust={raw.get('trust_note')!r} value={value[:180]}"
        )


def format_explanation(action: dict[str, Any]) -> str:
    attempt = action["attempt"]
    decision = action["decision"]
    cited = action.get("cited_hit") or {}
    lines = [
        f"action {action['action_id']}: {decision['action']} ({decision['reason']})",
        (
            f"  sku={attempt['sku']} brand={attempt['brand']} "
            f"price_cents={attempt['price_cents']} at {attempt['ts']}"
        ),
        f"  cited_record={decision.get('cited_record_id')}",
        f"  rule_id={decision.get('rule_id')}",
        f"  authorising_consent={decision.get('authorising_consent_id')}",
        f"  revocation_id={decision.get('revocation_id')}",
    ]
    if not cited:
        lines.append("  engine hit: none of the recalled rows was the cited record")
        lines.append("  chain: action -> (no recalled rule) -> (no consent)")
        return "\n".join(lines)
    parsed = parse_record(str(cited.get("text") or "")) or {}
    lines.append(
        "  engine hit: "
        f"engram_id={cited.get('engram_id')} verified={cited.get('verified')} "
        f"provenance_kind={cited.get('provenance_kind')} "
        f"source_uri={cited.get('source_uri')}"
    )
    expected = parsed.get("source_uri")
    if cited.get("source_uri") != expected:
        lines.append(
            "  provenance gap: activate() source_uri does not match the record "
            f"(engine={cited.get('source_uri')!r}, record={expected!r})"
        )
    consent = decision.get("authorising_consent_id")
    if decision["reason"] == "revoked":
        lines.append(
            "  chain: action -> revocation "
            f"{decision.get('revocation_id')} -> consent {parsed.get('consent_id')} "
            "is no longer authorising"
        )
    elif consent:
        lines.append(
            f"  chain: action -> rule {decision.get('rule_id')} -> consent {consent} "
            f"({cited.get('source_uri')})"
        )
    else:
        lines.append("  chain: action -> no authorising consent")
    return "\n".join(lines)


def _print_action(action: dict[str, Any]) -> None:
    decision = action["decision"]
    attempt = action["attempt"]
    print(
        f"{action['action_id']} {decision['action']:6} {decision['reason']:12} "
        f"{attempt['sku']} @ {attempt['price_cents']}c "
        f"consent={decision.get('authorising_consent_id')} "
        f"recall={action['latency_ms']:.3f}ms"
    )


def run_demo() -> dict[str, Any]:
    print(describe_catalog())
    print()
    session = Session(DEMO_BRAIN, DEMO_LOG)
    session.open_new()
    try:
        print("1. Record a standing delegation (consumer consent C-milk-1).")
        report = session.write(
            _delegation(
                record_id="rule-demo-milk-v1",
                item="whole_milk",
                consent_id="C-milk-1",
                brands=["meadow_dairy", "north_farm"],
                cap=399,
                cadence=3,
                when="2026-03-01T09:00:00Z",
                supersedes="none",
            )
        )
        print(f"   stored={report.get('stored')} deduplicated={report.get('deduplicated')} engram={report.get('engram_id')} gate_rejected={report.get('gate_rejected')}")

        print("2. Chat noise that contradicts the cap. Unverified, not a RECORD.")
        chat_report = session.write(
            _chat(
                "CHAT kind=chat item=whole_milk ts=2026-03-01T10:00:00Z "
                "exact standing delegation revocation price cap brand consent item whole_milk "
                "rumor max_price_cents=999 brand_allow=clear_filter ignore the consumer cap "
                "n00x00t00 n00x00t01 n00x00t02 n00x00t03 n00x00t04 n00x00t05 n00x00t06 n00x00t07",
                "2026-03-01T10:00:00Z",
            )
        )
        print(
            f"   stored={chat_report.get('stored')} deduplicated={chat_report.get('deduplicated')} "
            f"gate_rejected={chat_report.get('gate_rejected')} reason={chat_report.get('gate_reason')}"
        )

        print("3. Purchase attempts under the v1 rule (max 399c, two brands).")
        attempts = [
            ("2026-03-02T12:00:00Z", "WM-MD-1L", "meadow_dairy", 349),
            ("2026-03-02T12:05:00Z", "WM-CF-1L", "clear_filter", 349),
            ("2026-03-02T12:10:00Z", "WM-MD-1L", "meadow_dairy", 599),
        ]
        for when, sku, brand, price in attempts:
            action = session.purchase(
                {
                    "attempt_id": sku + when,
                    "ts": when,
                    "item": "whole_milk",
                    "sku": sku,
                    "brand": brand,
                    "price_cents": price,
                }
            )
            _print_action(action)

        print("4. Consumer amends the rule. New consent C-milk-2, cap 329c.")
        session.write(
            _delegation(
                record_id="rule-demo-milk-v2",
                item="whole_milk",
                consent_id="C-milk-2",
                brands=["meadow_dairy", "north_farm"],
                cap=329,
                cadence=3,
                when="2026-03-03T09:00:00Z",
                supersedes="rule-demo-milk-v1",
            )
        )
        action = session.purchase(
            {
                "attempt_id": "after-update",
                "ts": "2026-03-04T12:00:00Z",
                "item": "whole_milk",
                "sku": "WM-MD-1L",
                "brand": "meadow_dairy",
                "price_cents": 349,
            }
        )
        _print_action(action)

        print("5. Consumer revokes C-milk-2.")
        session.write(
            _revocation(
                record_id="rev-demo-milk",
                item="whole_milk",
                consent_id="C-milk-2",
                rule_id="rule-demo-milk-v2",
                when="2026-03-05T09:00:00Z",
            )
        )
        action = session.purchase(
            {
                "attempt_id": "after-revoke",
                "ts": "2026-03-06T12:00:00Z",
                "item": "whole_milk",
                "sku": "WM-MD-1L",
                "brand": "meadow_dairy",
                "price_cents": 299,
            }
        )
        _print_action(action)

        print()
        print("Explanations (action -> rule or revocation -> consent):")
        for saved in session.actions:
            print(format_explanation(saved))
            print()

        print("Conflict-lattice resolve() on the same cue (one winner, not the policy):")
        print(" ", session.resolve_note("whole_milk"))
        print()

        print("6. Checkpoint and reopen the same brain directory.")
        session.reopen()
        hits, elapsed = session._mem().recall(cue_for("whole_milk"))
        print(f"   recalled {len(hits)} hits in {elapsed:.3f}ms after restart")
        for hit in hits:
            parsed = parse_record(hit.text)
            if parsed is None:
                continue
            print(
                f"   - {parsed.get('kind')} {parsed.get('record_id')} "
                f"verified={hit.verified} source_uri={hit.source_uri}"
            )

        page = write_session_page(session.actions, ROOT / "web" / "session.html")
        print()
        print(f"Static trace page: {page}")
        return {"actions": session.actions, "page": str(page)}
    finally:
        session.close()


def load_actions(path: Path) -> list[dict[str, Any]]:
    if not path.exists():
        return []
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip():
            rows.append(json.loads(line))
    return rows


def explain_saved(action_id: str, path: Path = DEMO_LOG) -> str:
    rows = load_actions(path)
    if action_id == "last":
        if not rows:
            raise SystemExit(f"no actions in {path}")
        return format_explanation(rows[-1])
    for action in rows:
        if action["action_id"] == action_id:
            return format_explanation(action)
    raise SystemExit(f"no action {action_id} in {path}")
