"""Run the synthetic benchmark against three memories and write the raw numbers."""

from __future__ import annotations

import json
import platform
import shutil
import tempfile
from collections import defaultdict
from pathlib import Path
from typing import Any, Optional

from .dataset import DEFAULT_PATH, load_sessions
from .memory import Hit, fresh_memory, meta_for_event
from .policy import (
    DATASET_NAME,
    SEED,
    TOP_K,
    authoritative_record_id,
    cue_for,
    decide,
    parse_record,
    safety_ok,
    structured_records_before,
)

ROOT = Path(__file__).resolve().parents[1]
BACKENDS = ("fluctlight", "chat_log", "tfidf")


def percentile(values: list[float], p: float) -> Optional[float]:
    if not values:
        return None
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    rank = (len(ordered) - 1) * p
    lo = int(rank)
    hi = min(lo + 1, len(ordered) - 1)
    frac = rank - lo
    return ordered[lo] * (1.0 - frac) + ordered[hi] * frac


def _cpu_model() -> str:
    try:
        for line in Path("/proc/cpuinfo").read_text(encoding="utf-8").splitlines():
            if line.lower().startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor() or "unknown"


def _machine() -> dict[str, Any]:
    return {
        "python": platform.python_version(),
        "platform": platform.platform(),
        "system": platform.system(),
        "machine": platform.machine(),
        "cpu": _cpu_model(),
        "cpu_count": os_cpu(),
    }


def os_cpu() -> Optional[int]:
    import os

    return os.cpu_count()


def _hit_record_ids(hits: list[Hit]) -> list[str]:
    found: list[str] = []
    for hit in hits:
        parsed = parse_record(hit.text)
        if parsed is None:
            continue
        found.append(parsed["record_id"])
    return found


def _cited_hit(hits: list[Hit], record_id: Optional[str]) -> Optional[Hit]:
    if not record_id:
        return None
    for hit in hits:
        parsed = parse_record(hit.text)
        if parsed and parsed.get("record_id") == record_id:
            return hit
    return None


def run_backend(name: str, sessions: list[dict[str, Any]]) -> dict[str, Any]:
    latencies: list[float] = []
    rows: list[dict[str, Any]] = []
    ingest = {
        "stored": 0,
        "gate_rejected": 0,
        "deduplicated": 0,
        "chat_rejected": 0,
        "verified_rejected": 0,
    }
    by_cohort: dict[str, list[dict[str, bool]]] = defaultdict(list)

    for session in sessions:
        directory = tempfile.mkdtemp(prefix=f"shop-{name}-")
        memory = fresh_memory(name, directory)
        try:
            events = list(session["events"])
            cursor = 0
            seq = 0
            wrote_since_recall = True
            for attempt in session["attempts"]:
                wrote = False
                while cursor < len(events) and events[cursor]["ts"] <= attempt["ts"]:
                    event = events[cursor]
                    report = memory.add(
                        str(event["text"]),
                        meta=meta_for_event(event, session_id=session["session_id"], seq=seq),
                    )
                    seq += 1
                    cursor += 1
                    wrote = True
                    if name == "fluctlight":
                        if report.get("stored"):
                            ingest["stored"] += 1
                        if report.get("gate_rejected"):
                            ingest["gate_rejected"] += 1
                        if report.get("deduplicated"):
                            ingest["deduplicated"] += 1
                        if report.get("gate_rejected") and event.get("role") == "chat":
                            ingest["chat_rejected"] += 1
                        if report.get("gate_rejected") and event.get("role") != "chat":
                            ingest["verified_rejected"] += 1
                visible = structured_records_before(events, attempt["ts"])
                gt = dict(attempt["ground_truth"])
                recomputed = decide(visible, attempt)
                recomputed["authoritative_record_id"] = authoritative_record_id(visible, attempt)
                if recomputed != gt:
                    raise RuntimeError(
                        f"stored ground truth does not match policy for {attempt['attempt_id']}"
                    )
                hits, elapsed = memory.recall(cue_for(str(attempt["item"])), limit=TOP_K)
                latencies.append(elapsed)
                cold = wrote or wrote_since_recall
                wrote_since_recall = False
                recalled = []
                for hit in hits:
                    parsed = parse_record(hit.text)
                    if parsed is not None:
                        recalled.append(parsed)
                decision = decide(recalled, attempt)
                auth = gt.get("authoritative_record_id")
                ids = _hit_record_ids(hits)
                rule_hit = auth is not None and auth in ids
                cited = _cited_hit(hits, decision.get("cited_record_id"))
                trace_ok = (
                    decision["authorising_consent_id"] == gt.get("authorising_consent_id")
                    and decision["revocation_id"] == gt.get("revocation_id")
                )
                engine_uri_ok = None
                if name == "fluctlight" and cited is not None:
                    parsed = parse_record(cited.text) or {}
                    expected_uri = parsed.get("source_uri")
                    engine_uri_ok = bool(expected_uri) and cited.source_uri == expected_uri
                row = {
                    "session_id": session["session_id"],
                    "cohort": session["cohort"],
                    "attempt_id": attempt["attempt_id"],
                    "gt_action": gt["action"],
                    "gt_reason": gt["reason"],
                    "agent_action": decision["action"],
                    "agent_reason": decision["reason"],
                    "rule_recall": rule_hit,
                    "decision_match": decision["action"] == gt["action"],
                    "reason_match": decision["reason"] == gt["reason"],
                    "safety_ok": safety_ok(decision["action"], gt["action"]),
                    "trace_ok": trace_ok,
                    "engine_source_uri_ok": engine_uri_ok,
                    "authoritative_record_id": auth,
                    "cited_record_id": decision.get("cited_record_id"),
                    "recalled_record_ids": ids,
                    "latency_ms": round(elapsed, 4),
                    "cold_recall": cold,
                    "n_hits": len(hits),
                }
                rows.append(row)
                by_cohort[session["cohort"]].append(
                    {
                        "rule_recall": rule_hit,
                        "decision_match": row["decision_match"],
                        "safety_ok": row["safety_ok"],
                        "trace_ok": trace_ok,
                        "revoked": gt["reason"] == "revoked",
                        "agent_bought": decision["action"] == "buy",
                        "revocation_detected": gt["reason"] == "revoked"
                        and decision["reason"] == "revoked",
                    }
                )
        finally:
            memory.close()
            shutil.rmtree(directory, ignore_errors=True)

    summary = _summarize(rows, latencies)
    summary["ingest"] = ingest
    summary["by_cohort"] = {
        cohort: _summarize_flags(flags) for cohort, flags in sorted(by_cohort.items())
    }
    summary["failures"] = _failure_examples(rows)
    summary["n_failure_rows"] = sum(
        1
        for r in rows
        if (not r["rule_recall"])
        or (not r["decision_match"])
        or (not r["trace_ok"])
        or (not r["safety_ok"])
    )
    return summary


def _rate(rows: list[dict[str, Any]], key: str, subset=None) -> Optional[float]:
    chosen = rows if subset is None else [r for r in rows if subset(r)]
    if not chosen:
        return None
    return sum(1 for r in chosen if r[key]) / len(chosen)


def _summarize(rows: list[dict[str, Any]], latencies: list[float]) -> dict[str, Any]:
    revoked = [r for r in rows if r["gt_reason"] == "revoked"]
    illegal = [r for r in rows if r["agent_action"] == "buy" and r["gt_action"] != "buy"]
    uri_rows = [r for r in rows if r["engine_source_uri_ok"] is not None]
    cold = [float(r["latency_ms"]) for r in rows if r.get("cold_recall")]
    return {
        "n_attempts": len(rows),
        "rule_recall": _rate(rows, "rule_recall"),
        "policy_compliance": _rate(rows, "safety_ok"),
        "decision_accuracy": _rate(rows, "decision_match"),
        "reason_accuracy": _rate(rows, "reason_match"),
        "revocation_handling": _rate(revoked, "safety_ok") if revoked else None,
        "revocation_detected": _rate(revoked, "reason_match") if revoked else None,
        "n_revoked_attempts": len(revoked),
        "illegal_buys": len(illegal),
        "trace_correctness": _rate(rows, "trace_ok"),
        "engine_source_uri_match": (
            sum(1 for r in uri_rows if r["engine_source_uri_ok"]) / len(uri_rows) if uri_rows else None
        ),
        "n_engine_uri_checks": len(uri_rows),
        "latency_ms": {
            "n": len(latencies),
            "p50": percentile(latencies, 0.50),
            "p95": percentile(latencies, 0.95),
            "min": min(latencies) if latencies else None,
            "max": max(latencies) if latencies else None,
        },
        "latency_cold_ms": {
            "n": len(cold),
            "p50": percentile(cold, 0.50),
            "p95": percentile(cold, 0.95),
            "note": "Recall immediately after at least one new write. Repeated cues can hit FluctlightDB's activation cache; those calls are excluded here.",
        },
        "attempts": rows,
    }


def _summarize_flags(flags: list[dict[str, bool]]) -> dict[str, Any]:
    n = len(flags)
    revoked = [f for f in flags if f["revoked"]]

    def avg(rows: list[dict[str, bool]], key: str) -> Optional[float]:
        if not rows:
            return None
        return sum(1 for r in rows if r[key]) / len(rows)

    return {
        "n": n,
        "rule_recall": avg(flags, "rule_recall"),
        "decision_accuracy": avg(flags, "decision_match"),
        "policy_compliance": avg(flags, "safety_ok"),
        "trace_correctness": avg(flags, "trace_ok"),
        "n_revoked": len(revoked),
        "revocation_handling": (
            sum(1 for r in revoked if not r["agent_bought"]) / len(revoked) if revoked else None
        ),
        "revocation_detected": avg(revoked, "revocation_detected") if revoked else None,
    }


def _failure_examples(rows: list[dict[str, Any]], limit: int = 8) -> list[dict[str, Any]]:
    bad = [
        r
        for r in rows
        if (not r["rule_recall"]) or (not r["decision_match"]) or (not r["trace_ok"]) or (not r["safety_ok"])
    ]
    examples = []
    for row in bad[:limit]:
        examples.append(
            {
                "attempt_id": row["attempt_id"],
                "cohort": row["cohort"],
                "gt": f"{row['gt_action']}/{row['gt_reason']}",
                "agent": f"{row['agent_action']}/{row['agent_reason']}",
                "rule_recall": row["rule_recall"],
                "trace_ok": row["trace_ok"],
                "safety_ok": row["safety_ok"],
                "authoritative_record_id": row["authoritative_record_id"],
                "recalled_record_ids": row["recalled_record_ids"],
            }
        )
    return examples


def _fmt_rate(value: Optional[float]) -> str:
    if value is None:
        return "n/a"
    return f"{value * 100:.1f}%"


def _fmt_ms(value: Optional[float]) -> str:
    if value is None:
        return "n/a"
    return f"{value:.3f}"


def render_markdown(payload: dict[str, Any]) -> str:
    machine = payload["machine"]
    lines = [
        "# Shopping delegation benchmark results",
        "",
        "SYNTHETIC data only. These numbers are the output of one run on this machine.",
        "They do not describe real shoppers, and they are not a general claim about",
        "FluctlightDB versus other memory systems.",
        "",
        "## Machine",
        "",
        f"- Python {machine['python']}",
        f"- {machine['platform']}",
        f"- CPU: {machine['cpu']} ({machine['cpu_count']} logical CPUs)",
        f"- Dataset: `{payload['dataset']}` seed {payload['seed']}",
        f"- Sessions: {payload['n_sessions']} · attempts: {payload['n_attempts']} · recall limit: {payload['top_k']}",
        f"- SHA256 of `data/sessions.jsonl`: `{payload['data_sha256']}`",
        "",
        "## Headline",
        "",
        "Policy compliance here means the agent did not buy when the true policy",
        "forbids the purchase (price, brand, consent, revocation, cadence). A",
        "refusal is always compliant. Decision accuracy also requires the agent",
        "to buy when the standing rule allows it. Trace correctness means the",
        "cited authorising consent (or the revocation, when the rule is dead)",
        "matches ground truth.",
        "",
        "| memory | rule recall | policy compliance | decision accuracy | revocation handling | revocation detected | trace correctness | p50 ms | p95 ms | cold p50 ms | cold p95 ms |",
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for name in BACKENDS:
        summary = payload["backends"][name]
        lat = summary["latency_ms"]
        cold = summary["latency_cold_ms"]
        lines.append(
            "| {name} | {rr} | {pc} | {da} | {rh} | {rd} | {tr} | {p50} | {p95} | {c50} | {c95} |".format(
                name=name,
                rr=_fmt_rate(summary["rule_recall"]),
                pc=_fmt_rate(summary["policy_compliance"]),
                da=_fmt_rate(summary["decision_accuracy"]),
                rh=_fmt_rate(summary["revocation_handling"]),
                rd=_fmt_rate(summary["revocation_detected"]),
                tr=_fmt_rate(summary["trace_correctness"]),
                p50=_fmt_ms(lat["p50"]),
                p95=_fmt_ms(lat["p95"]),
                c50=_fmt_ms(cold["p50"]),
                c95=_fmt_ms(cold["p95"]),
            )
        )
    lines.extend(
        [
            "",
            "Illegal buys (agent bought, ground truth refused): "
            + ", ".join(
                f"{name}={payload['backends'][name]['illegal_buys']}" for name in BACKENDS
            )
            + ".",
            "",
            "`p50`/`p95` cover every recall. `cold` keeps only a recall that followed a write.",
            "FluctlightDB caches activation for a repeated cue, so the all-recall median can be",
            "a cache hit. Baselines recompute every time. Cold p95 is the fairer latency read.",
            "",
            "Fluctlight engine `source_uri` matched the cited record on "
            f"{_fmt_rate(payload['backends']['fluctlight']['engine_source_uri_match'])} "
            f"of {payload['backends']['fluctlight']['n_engine_uri_checks']} cited hits. "
            "Baselines have no engine provenance; their trace uses the consent id",
            "parsed from the retrieved line.",
            "",
            "## Ingest (Fluctlight only)",
            "",
            "The separation gate can refuse an unverified near-duplicate chat line.",
            "Verified delegations skip that gate. Baselines keep every line.",
            "",
            "```",
            json.dumps(payload["backends"]["fluctlight"]["ingest"], indent=2),
            "```",
            "",
            "## By cohort",
            "",
        ]
    )
    for name in BACKENDS:
        lines.append(f"### {name}")
        lines.append("")
        lines.append(
            "| cohort | n | rule recall | decision accuracy | policy compliance | trace | revoked n | revocation handling | revocation detected |"
        )
        lines.append("|---|---:|---:|---:|---:|---:|---:|---:|---:|")
        for cohort, stats in payload["backends"][name]["by_cohort"].items():
            lines.append(
                "| {c} | {n} | {rr} | {da} | {pc} | {tr} | {nr} | {rh} | {rd} |".format(
                    c=cohort,
                    n=stats["n"],
                    rr=_fmt_rate(stats["rule_recall"]),
                    da=_fmt_rate(stats["decision_accuracy"]),
                    pc=_fmt_rate(stats["policy_compliance"]),
                    tr=_fmt_rate(stats["trace_correctness"]),
                    nr=stats["n_revoked"],
                    rh=_fmt_rate(stats["revocation_handling"]),
                    rd=_fmt_rate(stats["revocation_detected"]),
                )
            )
        lines.append("")

    lines.extend(["## Where it misses", ""])
    for name in BACKENDS:
        failures = payload["backends"][name]["failures"]
        lines.append(f"### {name}")
        lines.append("")
        if not failures:
            lines.append("No missed recalls, decision mismatches, trace misses, or illegal buys in the sample.")
            lines.append("")
            continue
        lines.append(f"Showing {len(failures)} of {payload['backends'][name]['n_failure_rows']} misses.")
        lines.append("")
        for fail in failures:
            lines.append(
                f"- `{fail['attempt_id']}` ({fail['cohort']}): truth {fail['gt']}, "
                f"agent {fail['agent']}, rule_recall={fail['rule_recall']}, "
                f"trace_ok={fail['trace_ok']}, safety_ok={fail['safety_ok']}, "
                f"needed `{fail['authoritative_record_id']}`, "
                f"saw {fail['recalled_record_ids']}"
            )
        lines.append("")

    comparison = payload["comparison"]
    lines.extend(
        [
            "## Same, better, worse",
            "",
            f"- Fluctlight rule recall minus chat-log: {comparison['rule_recall_fluctlight_minus_chat_log']:+.1%}",
            f"- Fluctlight rule recall minus TF-IDF: {comparison['rule_recall_fluctlight_minus_tfidf']:+.1%}",
            f"- Fluctlight decision accuracy minus chat-log: {comparison['decision_fluctlight_minus_chat_log']:+.1%}",
            f"- Fluctlight decision accuracy minus TF-IDF: {comparison['decision_fluctlight_minus_tfidf']:+.1%}",
            f"- Attempts where chat-log recalled the authoritative record and Fluctlight did not: {comparison['chat_log_rule_hit_fluctlight_miss']}",
            f"- Attempts where TF-IDF recalled the authoritative record and Fluctlight did not: {comparison['tfidf_rule_hit_fluctlight_miss']}",
            f"- Attempts where Fluctlight recalled it and chat-log did not: {comparison['fluctlight_rule_hit_chat_log_miss']}",
            f"- Attempts where Fluctlight recalled it and TF-IDF did not: {comparison['fluctlight_rule_hit_tfidf_miss']}",
            "",
            "A positive delta means Fluctlight was higher on that metric in this run.",
            "A zero delta means the memories tied. On this set FluctlightDB matching TF-IDF",
            "is a tie, not a win. Latency is not a quality score. Cold p95 is the",
            "number to read: the all-recall median includes Fluctlight activation-cache hits,",
            "while the Python baselines scan the transcript every time.",
            "",
            "## What this does not show",
            "",
            "- Beta software, one embedded agent, one process, no network.",
            "- SYNTHETIC sessions from a fixed seed. Not a panel of shoppers.",
            "- The purchase policy is a deterministic rule, not a language model.",
            "- Provenance weighting in this demo is the engine's verified flag,",
            "  provenance kind, confidence, and source URI on `experience` / `activate`.",
            "  Exact-looking cues also hit `detect_exact_query`, which injects at most",
            "  three verified engrams ahead of associative hits. That is not a general",
            "  causal graph from action to consent.",
            "- `resolve()` returns a single provenance-weighted winner. The policy",
            "  needs the live rule, a possible revocation, and receipt history together,",
            "  so the measured path is `activate(limit=8)`, not `resolve()`.",
            "  On the scripted demo in this folder, `resolve()` on the same cue returned",
            "  the unverified chat line (`best match via ChatAssertion`), not the verified",
            "  delegation. Provenance weight did not outrank that cue-stuffed chat.",
            "- Cold recall p50 on this machine: FluctlightDB is slower than both baselines.",
            "  The all-recall median looks faster only because repeated cues hit the activation cache.",
            "- Results will move if the cue, the limit, the cohort mix, or the build changes.",
            "",
        ]
    )
    return "\n".join(lines)


def _delta(a: Optional[float], b: Optional[float]) -> float:
    return float(a or 0.0) - float(b or 0.0)


def compare(backends: dict[str, Any]) -> dict[str, Any]:
    by_id = {
        name: {row["attempt_id"]: row for row in summary["attempts"]}
        for name, summary in backends.items()
    }
    ids = list(by_id["fluctlight"])

    def count(winner: str, loser: str) -> int:
        n = 0
        for attempt_id in ids:
            if by_id[winner][attempt_id]["rule_recall"] and not by_id[loser][attempt_id]["rule_recall"]:
                n += 1
        return n

    fl = backends["fluctlight"]
    chat = backends["chat_log"]
    tfidf = backends["tfidf"]
    return {
        "rule_recall_fluctlight_minus_chat_log": _delta(fl["rule_recall"], chat["rule_recall"]),
        "rule_recall_fluctlight_minus_tfidf": _delta(fl["rule_recall"], tfidf["rule_recall"]),
        "decision_fluctlight_minus_chat_log": _delta(fl["decision_accuracy"], chat["decision_accuracy"]),
        "decision_fluctlight_minus_tfidf": _delta(fl["decision_accuracy"], tfidf["decision_accuracy"]),
        "chat_log_rule_hit_fluctlight_miss": count("chat_log", "fluctlight"),
        "tfidf_rule_hit_fluctlight_miss": count("tfidf", "fluctlight"),
        "fluctlight_rule_hit_chat_log_miss": count("fluctlight", "chat_log"),
        "fluctlight_rule_hit_tfidf_miss": count("fluctlight", "tfidf"),
    }


def run(data_path: Path = DEFAULT_PATH, out_dir: Optional[Path] = None) -> dict[str, Any]:
    import hashlib

    out_dir = out_dir or (ROOT / "results")
    out_dir.mkdir(parents=True, exist_ok=True)
    raw = data_path.read_bytes()
    sessions = load_sessions(data_path)
    payload: dict[str, Any] = {
        "synthetic": True,
        "label": "SYNTHETIC",
        "dataset": DATASET_NAME,
        "seed": SEED,
        "top_k": TOP_K,
        "n_sessions": len(sessions),
        "n_attempts": sum(len(s["attempts"]) for s in sessions),
        "data_sha256": hashlib.sha256(raw).hexdigest(),
        "machine": _machine(),
        "backends": {},
    }
    for name in BACKENDS:
        print(f"running {name} ...", flush=True)
        summary = run_backend(name, sessions)
        print(
            f"  rule_recall={summary['rule_recall']:.3f} "
            f"decision={summary['decision_accuracy']:.3f} "
            f"compliance={summary['policy_compliance']:.3f} "
            f"p50={summary['latency_ms']['p50']:.3f}ms",
            flush=True,
        )
        payload["backends"][name] = summary
    payload["comparison"] = compare(payload["backends"])
    json_path = out_dir / "benchmark.json"
    # Drop per-attempt rows from the markdown, keep them in JSON.
    json_path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    markdown = render_markdown(payload)
    results_md = ROOT / "RESULTS.md"
    results_md.write_text(markdown, encoding="utf-8")
    print(f"wrote {json_path}")
    print(f"wrote {results_md}")
    return payload
