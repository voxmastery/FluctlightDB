#!/usr/bin/env python3
"""Isolate activate/recall latency vs full LongMemEval bench wall time.

Photon LSH + Fabric rerank on the hot path are sub-millisecond per query on a warm brain.
~60s/question in brain-mode benchmarks is dominated by embedding HTTP, haystack ingest,
and sleep — not activate().

Usage:
  python benchmarks/profile_recall_path.py
  FLUCTLIGHT_SERVE_URL=https://search.ambugo.help/brain python benchmarks/profile_recall_path.py
"""

from __future__ import annotations

import os
import statistics
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "sdks" / "python"))

from fluctlightdb import FluctlightClient  # noqa: E402


def ms(fn, *a, **kw) -> tuple[float, object]:
    t0 = time.perf_counter()
    out = fn(*a, **kw)
    return (time.perf_counter() - t0) * 1000.0, out


def main() -> None:
    client = FluctlightClient.from_env()
    if not client.health():
        print("Fluctlight serve not reachable — set FLUCTLIGHT_SERVE_URL / FLUCTLIGHT_API_KEY")
        sys.exit(1)

    st = client.status()
    engrams = st.get("engrams", st.get("engram_count", "?"))
    print(f"brain engrams={engrams} url={client.base_url}")

    cue = os.environ.get("PROFILE_CUE", "What is the user's favorite color?")
    n = int(os.environ.get("PROFILE_N", "20"))

    # Warm HTTP + brain caches
    client.activate_lite(cue)

    activate_ms: list[float] = []
    lite_ms: list[float] = []
    for _ in range(n):
        dt, _ = ms(client.activate, cue, limit=8)
        activate_ms.append(dt)
        dt2, _ = ms(client.activate_lite, cue)
        lite_ms.append(dt2)

    def summary(name: str, xs: list[float]) -> None:
        xs_sorted = sorted(xs)
        p50 = statistics.median(xs_sorted)
        p95 = xs_sorted[int(0.95 * (len(xs_sorted) - 1))]
        print(
            f"{name:16} n={len(xs)}  "
            f"min={min(xs):.2f}ms  p50={p50:.2f}ms  p95={p95:.2f}ms  max={max(xs):.2f}ms"
        )

    print()
    print("=== HTTP activate latency (warm brain, no ingest) ===")
    summary("activate(limit=8)", activate_ms)
    summary("activate_lite", lite_ms)

    print()
    print("LongMemEval brain mode ~60s/q includes ~600 experience()+embed calls + sleep.")
    print("Photon/Fabric work lives inside activate — numbers above are the real hot path.")


if __name__ == "__main__":
    main()
