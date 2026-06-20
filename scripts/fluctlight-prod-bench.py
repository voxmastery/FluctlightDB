#!/usr/bin/env python3
"""Production benchmark: FluctlightDB vs SQLite lexical vs brute vector scan."""

from __future__ import annotations

import json
import math
import os
import sqlite3
import statistics
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

SDK = Path(__file__).resolve().parents[1] / "sdks" / "python"
if SDK.is_dir() and str(SDK) not in sys.path:
    sys.path.insert(0, str(SDK))

try:
    from fluctlightdb import FluctlightClient
except ImportError:
    FluctlightClient = None  # type: ignore

SERVE = os.environ.get("FLUCTLIGHT_SERVE_URL", "http://127.0.0.1:8792").rstrip("/")
API_KEY = os.environ.get("FLUCTLIGHT_API_KEY", "")
BRAIN = Path(os.environ.get("FLUCTLIGHT_BRAIN", Path.home() / ".fluctlight/tenants/default/brain"))

PAIRS = [
    ("database connection pool exhausted", "db pool timeout", [0.9, 0.1, 0.0]),
    ("redis cache miss storm", "cache invalidation spike", [0.85, 0.15, 0.0]),
    ("kubernetes pod crash loop", "k8s container restart loop", [0.88, 0.12, 0.0]),
    ("payment webhook signature invalid", "stripe webhook auth failed", [0.92, 0.08, 0.0]),
    ("user login brute force", "account lockout threshold", [0.8, 0.2, 0.0]),
]

WALLET_CUES = ["wallet balance", "my balance", "what is my balance"]


def cosine(a: list[float], b: list[float]) -> float:
    dot = sum(x * y for x, y in zip(a, b))
    na = math.sqrt(sum(x * x for x in a))
    nb = math.sqrt(sum(x * x for x in b))
    return dot / (na * nb) if na and nb else 0.0


def pct(values: list[float], p: float) -> float:
    if not values:
        return 0.0
    s = sorted(values)
    i = int(round((len(s) - 1) * p))
    return s[i]


def load_corpus_from_export() -> list[dict]:
    """Load engrams from export-raw via HTTP or CLI."""
    if FluctlightClient is not None:
        client = FluctlightClient(base_url=SERVE, api_key=API_KEY)
        if client.health():
            try:
                raw = client.export_raw()
                return raw.get("engrams") or []
            except Exception:
                pass
    bin_path = Path(os.environ.get("FLUCTLIGHT_BIN", "/home/ambugo/fluctlightdb/target/release/fluctlight"))
    if bin_path.is_file():
        import subprocess

        out = subprocess.run(
            [str(bin_path), "export-raw", "--path", str(BRAIN)],
            capture_output=True,
            text=True,
            timeout=120,
        )
        if out.returncode == 0 and out.stdout.strip():
            data = json.loads(out.stdout)
            return data.get("engrams") or []
    return []


def sqlite_lexical_bench(engrams: list[dict], cues: list[str], n: int = 100) -> dict:
    conn = sqlite3.connect(":memory:")
    conn.execute("CREATE TABLE memories (id INTEGER PRIMARY KEY, content TEXT, vec BLOB)")
    rows = []
    for i, e in enumerate(engrams):
        ep = e.get("episode") or {}
        content = (ep.get("content") or "").strip()
        if not content:
            continue
        vec = ep.get("semantic_vector")
        rows.append((content, json.dumps(vec) if vec else None))
    conn.executemany("INSERT INTO memories(content, vec) VALUES (?, ?)", rows)
    conn.commit()

    latencies: list[float] = []
    hits = 0
    for _ in range(n):
        for cue in cues:
            t0 = time.perf_counter()
            cur = conn.execute(
                "SELECT content FROM memories WHERE lower(content) LIKE ? OR lower(content) LIKE ? LIMIT 1",
                (f"%{cue.lower()}%", f"%{cue.split()[0].lower()}%"),
            )
            row = cur.fetchone()
            latencies.append((time.perf_counter() - t0) * 1000)
            if row:
                hits += 1
    return {
        "queries": n * len(cues),
        "hits": hits,
        "p50_ms": round(pct(latencies, 0.5), 4),
        "p95_ms": round(pct(latencies, 0.95), 4),
        "avg_ms": round(statistics.mean(latencies) if latencies else 0, 4),
    }


def vector_brute_bench(engrams: list[dict], pairs: list[tuple], n: int = 100) -> dict:
    corpus: list[tuple[str, list[float] | None]] = []
    for e in engrams:
        ep = e.get("episode") or {}
        content = (ep.get("content") or "").strip()
        vec = ep.get("semantic_vector")
        if content and vec:
            corpus.append((content, vec))

    latencies: list[float] = []
    hits = 0
    for _ in range(n):
        for _, cue, vec in pairs:
            t0 = time.perf_counter()
            best = 0.0
            for _, stored in corpus:
                if stored:
                    best = max(best, cosine(vec, stored))
            latencies.append((time.perf_counter() - t0) * 1000)
            if best >= 0.75:
                hits += 1
    return {
        "queries": n * len(pairs),
        "hits": hits,
        "p50_ms": round(pct(latencies, 0.5), 4),
        "p95_ms": round(pct(latencies, 0.95), 4),
        "avg_ms": round(statistics.mean(latencies) if latencies else 0, 4),
        "corpus_vectors": len(corpus),
    }


def fluctlight_http_bench(client: FluctlightClient, pairs: list[tuple], wallet_cues: list[str], n: int = 50) -> dict:
    latencies: list[float] = []
    hits = 0
    for _ in range(n):
        for _, cue, vec in pairs:
            t0 = time.perf_counter()
            try:
                r = client.activate(cue, semantic_vector=vec)
                latencies.append((time.perf_counter() - t0) * 1000)
                if (r.get("recalls") or []):
                    hits += 1
            except Exception:
                latencies.append((time.perf_counter() - t0) * 1000)

    wallet = {}
    for cue in wallet_cues:
        try:
            r = client.activate(cue)
            top = (r.get("recalls") or [{}])[0]
            ep = top.get("episode") or {}
            wallet[cue] = {
                "verified": top.get("verified"),
                "content": (ep.get("content") or "")[:120],
                "activation": top.get("activation"),
            }
        except Exception as e:
            wallet[cue] = {"error": str(e)}

    status = client.status()
    stage = client.stage_report()
    verified = client.verified_context(limit=8)

    return {
        "queries": n * len(pairs),
        "hits": hits,
        "p50_ms": round(pct(latencies, 0.5), 4),
        "p95_ms": round(pct(latencies, 0.95), 4),
        "avg_ms": round(statistics.mean(latencies) if latencies else 0, 4),
        "wallet_recall": wallet,
        "brain": status,
        "stage": stage,
        "verified_facts": len(verified.get("facts") or []),
        "unverified_warnings": len(verified.get("unverified_warnings") or []),
    }


def brain_size_mb() -> float:
    if not BRAIN.exists():
        return 0.0
    total = 0
    for p in BRAIN.rglob("*"):
        if p.is_file():
            total += p.stat().st_size
    return round(total / (1024 * 1024), 2)


def main() -> int:
    print("=== FluctlightDB production benchmark ===\n")
    engrams = load_corpus_from_export()
    print(f"Production brain path: {BRAIN}")
    print(f"Engrams loaded: {len(engrams)}")
    print(f"On-disk brain size: {brain_size_mb()} MB\n")

    cues = [c for _, c, _ in PAIRS]
    sqlite_stats = sqlite_lexical_bench(engrams, cues, n=80)
    vector_stats = vector_brute_bench(engrams, PAIRS, n=80)

    fl_stats = {"error": "serve unavailable"}
    if FluctlightClient is not None:
        client = FluctlightClient(base_url=SERVE, api_key=API_KEY)
        if client.health():
            fl_stats = fluctlight_http_bench(client, PAIRS, WALLET_CUES, n=40)
        else:
            fl_stats = {"error": f"health check failed at {SERVE}"}

    report = {
        "fluctlight": fl_stats,
        "sqlite_lexical": sqlite_stats,
        "vector_brute_force": vector_stats,
        "engrams": len(engrams),
        "brain_mb": brain_size_mb(),
    }
    print(json.dumps(report, indent=2))

    out = Path(__file__).with_name("prod-bench-latest.json")
    out.write_text(json.dumps(report, indent=2))
    print(f"\nWrote {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
