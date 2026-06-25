"""Cross-process write lock for embedded brain directories."""

from __future__ import annotations

import fcntl
import os
from contextlib import contextmanager
from pathlib import Path
from typing import Iterator


@contextmanager
def brain_write_lock(brain_dir: str | os.PathLike[str], *, timeout_s: float = 30.0) -> Iterator[None]:
    """Serialize writes to a brain directory (multiple agents / MCP / hooks)."""
    path = Path(brain_dir)
    path.mkdir(parents=True, exist_ok=True)
    lock_path = path / ".brain.lock"
    with open(lock_path, "a+", encoding="utf-8") as fh:
        fh.write("")
        fh.flush()
        if timeout_s <= 0:
            fcntl.flock(fh.fileno(), fcntl.LOCK_EX)
        else:
            import time

            deadline = time.time() + timeout_s
            while True:
                try:
                    fcntl.flock(fh.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
                    break
                except BlockingIOError:
                    if time.time() >= deadline:
                        raise TimeoutError(f"brain write lock timeout: {lock_path}")
                    time.sleep(0.05)
        try:
            yield
        finally:
            fcntl.flock(fh.fileno(), fcntl.LOCK_UN)
