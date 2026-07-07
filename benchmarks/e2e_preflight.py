#!/usr/bin/env python3
"""Pre-flight checks before LongMemEval E2E 500 — run without API key."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
SDK = REPO / "sdks" / "python"
if str(SDK) not in sys.path:
    sys.path.insert(0, str(SDK))
sys.path.insert(0, str(REPO / "benchmarks"))

DEFAULT_DATA = Path("/tmp/longmemeval/data/longmemeval_s_cleaned.json")


def check(name: str, ok: bool, detail: str = "") -> bool:
    mark = "OK" if ok else "FAIL"
    line = f"  [{mark}] {name}"
    if detail:
        line += f" — {detail}"
    print(line)
    return ok


def main() -> int:
    print("FluctlightDB E2E pre-flight\n")
    all_ok = True

    # Dataset
    all_ok &= check("dataset", DEFAULT_DATA.is_file(), str(DEFAULT_DATA))

    # Native extension
    try:
        import fluctlightdb_native  # noqa: F401

        native_ok = True
        detail = "fluctlightdb-native installed"
    except ImportError as e:
        native_ok = False
        detail = str(e)[:80]
    all_ok &= check("native", native_ok, detail)

    # Muon connect
    if native_ok:
        try:
            from fluctlightdb import connect_muon

            b = connect_muon()
            muon_ok = hasattr(b, "muon_imprint_batch")
            all_ok &= check("muon lane", muon_ok)
        except Exception as e:
            all_ok &= check("muon lane", False, str(e)[:80])

    # Embed server (optional for muon path)
    embed_url = os.environ.get("FLUCTLIGHT_EMBED_URL", "http://127.0.0.1:8793")
    try:
        req = urllib.request.Request(f"{embed_url.rstrip('/')}/health", method="GET")
        with urllib.request.urlopen(req, timeout=3) as resp:
            embed_ok = resp.status == 200
    except Exception:
        embed_ok = False
    check(
        "embed server",
        embed_ok,
        f"{embed_url} (optional for muon v4; required for index/brain ingest)",
    )

    # API keys (warn only)
    from cloud_llm import load_env_file

    load_env_file()
    has_openai = bool(os.environ.get("OPENAI_API_KEY", "").strip())
    has_gemini = bool(os.environ.get("GEMINI_API_KEY", "").strip())
    check("OPENAI_API_KEY", has_openai, "required for paper-credible gpt-4o run")
    check("GEMINI_API_KEY", has_gemini, "optional fallback")

    # Quick retrieval smoke (muon, 3 questions, no LLM)
    if native_ok and DEFAULT_DATA.is_file():
        print("\n  Running muon retrieval smoke (3 questions)...")
        cmd = [
            sys.executable,
            str(REPO / "benchmarks" / "longmemeval_bench.py"),
            "--mode",
            "brain",
            "--muon",
            "--granularity",
            "session",
            "--metric",
            "session",
            "--top-k",
            "8",
            "--limit",
            "3",
            "--dual-key",
            "--query-expand",
            "--pref-facts-key",
            "--json-out",
            "/tmp/fluctlight-e2e-preflight.json",
        ]
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
        if proc.returncode == 0 and Path("/tmp/fluctlight-e2e-preflight.json").is_file():
            data = json.loads(Path("/tmp/fluctlight-e2e-preflight.json").read_text())
            summ = data.get("summary", data)
            hit = summ.get("session_recall_at_k", 0)
            sec = summ.get("sec_per_question", 0)
            all_ok &= check(
                "muon retrieval smoke",
                hit >= 1.0,
                f"session@8={hit:.0%} sec/q={sec:.2f}",
            )
        else:
            err = (proc.stderr or proc.stdout or "")[-200:]
            all_ok &= check("muon retrieval smoke", False, err)

    print()
    if all_ok:
        print("READY for E2E 500. Recommended command:")
        print(
            "  E2E_PROFILE=paper E2E_LIMIT=500 E2E_BACKEND=openai \\\n"
            "  PYTHONUNBUFFERED=1 benchmarks/e2e_certify.sh"
        )
        return 0
    print("NOT READY — fix failures above before spending API credits.")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
