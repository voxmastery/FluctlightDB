"""Cross-platform helpers for FluctlightDB project brains."""

from __future__ import annotations

import os
import shutil
import sys
from pathlib import Path


def is_windows() -> bool:
    return os.name == "nt" or sys.platform.startswith("win")


def normalize_subdir(path: str) -> str:
    return path.replace("\\", "/").strip("/")


def brain_lock_path(brain_path: str | os.PathLike[str]) -> Path:
    """Match Rust ``storage::lock_path`` (v4 dir → ``.brain.lock``)."""
    p = Path(brain_path)
    if p.suffix == ".flct":
        return p.with_suffix(".flct.lock")
    if p.is_dir() or (p / "manifest.json").is_file():
        return p / ".brain.lock"
    # Project brains and new embedded paths are v4 directories.
    return p / ".brain.lock"


def python_for_mcp() -> str:
    """Python executable for MCP server configs (Windows-friendly)."""
    if is_windows():
        py = shutil.which("py")
        if py:
            return py
        return sys.executable
    for candidate in (shutil.which("python3"), shutil.which("python"), sys.executable):
        if candidate:
            return candidate
    return "python3"


def python_mcp_args() -> list[str]:
    """Command + args for MCP JSON (``py -3`` on Windows when using launcher)."""
    if is_windows():
        py = shutil.which("py")
        if py:
            return [py, "-3"]
    return [python_for_mcp()]
