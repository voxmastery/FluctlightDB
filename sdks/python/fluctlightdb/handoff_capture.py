"""Git-aware session capture for rich handoffs."""

from __future__ import annotations

import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional


@dataclass
class SessionCapture:
    summary: str = ""
    files: list[str] = field(default_factory=list)
    next_steps: list[str] = field(default_factory=list)
    branch: str = ""
    git_available: bool = False


def _run_git(root: Path, *args: str, timeout: float = 5.0) -> Optional[str]:
    try:
        proc = subprocess.run(
            ["git", *args],
            cwd=root,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
        if proc.returncode != 0:
            return None
        return proc.stdout.strip()
    except (OSError, subprocess.TimeoutExpired):
        return None


def _is_git_repo(root: Path) -> bool:
    return _run_git(root, "rev-parse", "--is-inside-work-tree") == "true"


def capture_session_summary(root: Path, *, hook_summary: str = "") -> SessionCapture:
    """Build handoff fields from git state + optional hook payload."""
    cap = SessionCapture()
    if hook_summary.strip():
        cap.summary = hook_summary.strip()[:1200]

    if not _is_git_repo(root):
        if not cap.summary:
            cap.summary = "Session ended — not a git repository."
        cap.next_steps = ["Review recent edits and continue from handoff context."]
        return cap

    cap.git_available = True
    branch = _run_git(root, "branch", "--show-current") or ""
    cap.branch = branch

    stat = _run_git(root, "diff", "--stat", "HEAD") or ""
    names = _run_git(root, "diff", "--name-only", "HEAD") or ""
    staged_names = _run_git(root, "diff", "--name-only", "--cached") or ""
    unstaged = _run_git(root, "diff", "--name-only") or ""

    files: list[str] = []
    for blob in (names, staged_names, unstaged):
        for line in blob.splitlines():
            line = line.strip().replace("\\", "/")
            if line and line not in files:
                files.append(line)
    cap.files = files[:40]

    subject = _run_git(root, "log", "-1", "--pretty=%s") or ""
    parts: list[str] = []
    if branch:
        parts.append(f"Branch: {branch}")
    if subject:
        parts.append(f"Last commit: {subject[:200]}")
    if stat:
        parts.append(f"Changes:\n{stat[:800]}")
    elif cap.files:
        parts.append(f"Touched {len(cap.files)} file(s)")
    if hook_summary.strip() and hook_summary.strip() not in "\n".join(parts):
        parts.append(hook_summary.strip()[:600])

    if parts:
        cap.summary = "\n".join(parts)[:1200]
    elif not cap.summary:
        cap.summary = "Session ended with no uncommitted changes detected."

    steps: list[str] = []
    if cap.files:
        steps.append(f"Review changes in: {', '.join(cap.files[:6])}")
    if branch:
        steps.append(f"Continue on branch `{branch}`")
    steps.append("Read recent handoffs via fluctlight_list_handoffs / session_context.")
    cap.next_steps = steps[:6]
    return cap
