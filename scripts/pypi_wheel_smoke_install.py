#!/usr/bin/env python3
"""Cross-platform PyPI wheel smoke install (mirrors CI pypi-wheel-smoke)."""

from __future__ import annotations

import glob
import os
import platform
import shutil
import subprocess
import sys
import venv
from pathlib import Path


def _pick_wheel(pattern: str) -> Path:
    matches = sorted(Path(p) for p in glob.glob(pattern))
    if not matches:
        raise FileNotFoundError(f"No wheel matched {pattern!r}")

    # SDK is pure Python; native wheel must match host OS/arch.
    if "dist-native" in pattern.replace("\\", "/"):
        return _pick_native_wheel(matches)
    # dist-sdk: prefer platform-independent wheel
    for m in reversed(matches):
        if "py3-none-any" in m.name or "any" in m.name.split("-")[-1]:
            return m
    return matches[-1]


def _pick_native_wheel(matches: list[Path]) -> Path:
    """Select abi3 native wheel for the current host (avoid manylinux on Windows)."""
    abi = [m for m in matches if "abi3" in m.name]
    pool = abi or matches

    if sys.platform == "win32":
        machine = platform.machine().lower()
        if machine in ("arm64", "aarch64"):
            tags = ("win_arm64", "win_amd64", "win32")
        else:
            tags = ("win_amd64", "win32", "win_arm64")
    elif sys.platform == "darwin":
        tags = ("macosx_11_0_universal2", "macosx_10_9_universal2", "macosx", "darwin")
    else:
        tags = ("manylinux", "linux")

    for tag in tags:
        for m in reversed(pool):
            if tag in m.name:
                return m
    return pool[-1]


def _pip(py: Path, *args: str) -> None:
    """Always invoke pip as ``python -m pip`` (required on Windows venvs)."""
    cmd = [str(py), "-m", "pip", *args]
    subprocess.check_call(cmd)


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    native = _pick_wheel(str(root / "dist-native" / "*.whl"))
    sdk = _pick_wheel(str(root / "dist-sdk" / "*.whl"))
    print(f"selected native={native.name}")
    print(f"selected sdk={sdk.name}")

    smoke_root = Path(os.environ.get("RUNNER_TEMP", "/tmp"))
    vdir = smoke_root / "flct-smoke"
    if vdir.exists():
        shutil.rmtree(vdir)

    venv.create(vdir, with_pip=True)
    py = vdir / ("Scripts/python.exe" if os.name == "nt" else "bin/python")

    # Upgrade is optional — Windows rejects `pip.exe install --upgrade pip`
    # and requires `python -m pip`; skip hard-fail if upgrade is blocked.
    try:
        _pip(py, "install", "--upgrade", "pip")
    except subprocess.CalledProcessError as exc:
        print(f"WARN: pip upgrade skipped ({exc})", file=sys.stderr)

    _pip(py, "install", "--force-reinstall", str(native), str(sdk))
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
