#!/usr/bin/env python3
"""Cross-platform PyPI wheel smoke install (mirrors CI pypi-wheel-smoke)."""

from __future__ import annotations

import glob
import os
import shutil
import subprocess
import sys
import venv
from pathlib import Path


def _pick_wheel(pattern: str) -> Path:
    matches = sorted(Path(p) for p in glob.glob(pattern))
    if not matches:
        raise FileNotFoundError(f"No wheel matched {pattern!r}")
    return matches[-1]


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    native = _pick_wheel(str(root / "dist-native" / "*.whl"))
    sdk = _pick_wheel(str(root / "dist-sdk" / "*.whl"))

    smoke_root = Path(os.environ.get("RUNNER_TEMP", "/tmp"))
    vdir = smoke_root / "flct-smoke"
    if vdir.exists():
        shutil.rmtree(vdir)

    venv.create(vdir, with_pip=True)
    py = vdir / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
    pip = vdir / ("Scripts/pip.exe" if os.name == "nt" else "bin/pip")

    subprocess.check_call([str(pip), "install", "--upgrade", "pip"])
    subprocess.check_call([str(pip), "install", str(native), str(sdk)])
    subprocess.check_call(
        [
            str(py),
            "-c",
            "import fluctlightdb_native; import fluctlightdb; "
            "print('ok', fluctlightdb_native.__name__)",
        ]
    )
    print(f"PASS native={native.name} sdk={sdk.name}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        raise
