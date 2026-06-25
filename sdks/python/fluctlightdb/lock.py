"""Cross-process write lock for embedded brain directories."""

from __future__ import annotations

import os
from contextlib import contextmanager
from pathlib import Path
from typing import Iterator

from filelock import Timeout
from filelock import FileLock as _FileLock

from .platform import brain_lock_path


@contextmanager
def file_write_lock(lock_file: str | os.PathLike[str], *, timeout_s: float = 30.0) -> Iterator[None]:
    """Serialize writes on an arbitrary lock file path."""
    path = Path(lock_file)
    path.parent.mkdir(parents=True, exist_ok=True)
    lock = _FileLock(str(path), timeout=-1 if timeout_s <= 0 else timeout_s)
    try:
        lock.acquire()
        yield
    except Timeout as exc:
        raise TimeoutError(f"file write lock timeout: {path}") from exc
    finally:
        if lock.is_locked:
            lock.release()


@contextmanager
def brain_write_lock(brain_dir: str | os.PathLike[str], *, timeout_s: float = 30.0) -> Iterator[None]:
    """Serialize writes to a brain directory (multiple agents / MCP / hooks)."""
    path = Path(brain_dir)
    path.mkdir(parents=True, exist_ok=True)
    lock_file = brain_lock_path(path)
    lock = _FileLock(str(lock_file), timeout=-1 if timeout_s <= 0 else timeout_s)
    try:
        lock.acquire()
        yield
    except Timeout as exc:
        raise TimeoutError(f"brain write lock timeout: {lock_file}") from exc
    finally:
        if lock.is_locked:
            lock.release()
