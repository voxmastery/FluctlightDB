#!/usr/bin/env python3
"""Run 10k engram LongMemEval-style scale benchmark (Rust test)."""
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
env = os.environ.copy()
env.setdefault("FLUCTLIGHT_SCALE_BENCH", "1")
env.setdefault("FLUCTLIGHT_SCALE_N", "10000")
env.setdefault("FLUCTLIGHT_SCALE_TARGET", "0.85")

r = subprocess.run(
    [
        "cargo",
        "test",
        "-p",
        "fluctlightdb",
        "scale_recall_10k",
        "--",
        "--ignored",
        "--nocapture",
    ],
    cwd=ROOT,
    env=env,
)
sys.exit(r.returncode)
