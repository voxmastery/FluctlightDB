#!/usr/bin/env python3
"""Profile Muon Lane bulk imprint vs haystack experience() ingest."""

from __future__ import annotations

import os
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "sdks" / "python"))

os.environ["FLUCTLIGHT_MUON"] = "1"

from fluctlightdb import connect_muon  # noqa: E402


def main() -> None:
    brain = connect_muon()
    sessions = []
    for i in range(50):
        sessions.append(
            {
                "session_id": f"s{i}",
                "date": "2023-06-01",
                "body": f"user: I discussed topic {i} and item {i} in detail.\n"
                f"assistant: Thanks for sharing about topic {i}.",
                "user_keys": f"user: topic {i} item {i}",
            }
        )
    t0 = time.perf_counter()
    n = brain.muon_imprint_batch(sessions)
    imprint_ms = (time.perf_counter() - t0) * 1000.0

    t1 = time.perf_counter()
    hits = brain.muon_recall("What item did I discuss in topic 7?", limit=5)
    recall_ms = (time.perf_counter() - t1) * 1000.0

    print(f"muon imprint 50 sessions: {imprint_ms:.2f}ms ({n} sessions)")
    print(f"muon recall:              {recall_ms:.2f}ms top={hits[0]['session_id'] if hits else '?'}")
    print(f"muon_len={brain.muon_len()}")
    print("Haystack brain mode ~600 experience()+embed calls ≈ 60s/question.")


if __name__ == "__main__":
    main()
