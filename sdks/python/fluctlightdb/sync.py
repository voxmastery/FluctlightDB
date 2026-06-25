"""Git-based team sync for shared project brains and handoff inbox."""

from __future__ import annotations

import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

from .project import CONFIG_FILE, FLUCTLIGHT_DIR, find_project_root

SYNC_PATHS = (
    ".fluctlight/config.yaml",
    ".fluctlight/handoffs.jsonl",
    ".fluctlight/project",
    ".fluctlight/TEAM_SYNC.md",
)


@dataclass
class SyncResult:
    ok: bool
    message: str
    stdout: str = ""
    stderr: str = ""


def _git(root: Path, *args: str, timeout: float = 120.0) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=root,
        capture_output=True,
        text=True,
        timeout=timeout,
        check=False,
    )


def _is_repo(root: Path) -> bool:
    return _git(root, "rev-parse", "--is-inside-work-tree").stdout.strip() == "true"


def sync_status(start: Optional[Path] = None) -> SyncResult:
    root = find_project_root(start)
    if not _is_repo(root):
        return SyncResult(False, "not a git repository")
    dirty = _git(root, "status", "--porcelain", *SYNC_PATHS)
    if dirty.stdout.strip():
        return SyncResult(True, "local changes pending", stdout=dirty.stdout.strip())
    ahead = _git(root, "rev-list", "--count", "@{u}..HEAD")
    behind = _git(root, "rev-list", "--count", "HEAD..@{u}")
    msg = f"synced (ahead {ahead.stdout.strip()}, behind {behind.stdout.strip()})"
    return SyncResult(True, msg)


def sync_pull(start: Optional[Path] = None, *, rebase: bool = False) -> SyncResult:
    root = find_project_root(start)
    if not _is_repo(root):
        return SyncResult(False, "not a git repository — run inside your monorepo")
    cmd = ["pull", "--rebase"] if rebase else ["pull"]
    proc = _git(root, *cmd)
    if proc.returncode != 0:
        return SyncResult(False, "git pull failed", stdout=proc.stdout, stderr=proc.stderr)
    return SyncResult(True, "pulled shared brain + handoffs", stdout=proc.stdout.strip())


def sync_push(
    start: Optional[Path] = None,
    *,
    message: str = "chore(fluctlight): sync project brain and handoffs",
    auto_commit: bool = True,
) -> SyncResult:
    root = find_project_root(start)
    if not _is_repo(root):
        return SyncResult(False, "not a git repository — run inside your monorepo")

    cfg = root / FLUCTLIGHT_DIR / CONFIG_FILE
    if cfg.is_file():
        text = cfg.read_text(encoding="utf-8")
        if "team_sync: true" not in text and "team_sync: true" not in text.replace(" ", ""):
            pass  # still allow push if paths exist

    if auto_commit:
        _git(root, "add", *SYNC_PATHS)
        status = _git(root, "status", "--porcelain", *SYNC_PATHS)
        if not status.stdout.strip():
            return SyncResult(True, "nothing to commit")
        commit = _git(root, "commit", "-m", message)
        if commit.returncode != 0 and "nothing to commit" not in commit.stdout:
            return SyncResult(False, "git commit failed", stderr=commit.stderr)

    push = _git(root, "push")
    if push.returncode != 0:
        return SyncResult(False, "git push failed", stdout=push.stdout, stderr=push.stderr)
    return SyncResult(True, "pushed shared brain + handoffs", stdout=push.stdout.strip())
