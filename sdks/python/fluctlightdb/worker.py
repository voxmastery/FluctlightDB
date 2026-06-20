"""Persistent in-process recall — native library (best) or worker subprocess."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import threading
from pathlib import Path
from typing import Any, Optional, Protocol


def _default_fluctlight_bin() -> str:
    env = os.environ.get("FLUCTLIGHT_BIN")
    if env:
        return env
    repo_root = Path(__file__).resolve().parents[3]
    release = repo_root / "target" / "release" / "fluctlight"
    if release.is_file():
        return str(release)
    debug = repo_root / "target" / "debug" / "fluctlight"
    if debug.is_file():
        return str(debug)
    found = shutil.which("fluctlight")
    return found if found else "fluctlight"


class RecallClient(Protocol):
    def activate(
        self,
        cue: str,
        semantic_vector: Optional[list[float]] = None,
        agent_id: Optional[str] = None,
        limit: Optional[int] = None,
    ) -> dict[str, Any]: ...

    def activate_batch(
        self,
        items: list[dict[str, Any]],
        limit: Optional[int] = None,
    ) -> dict[str, Any]: ...

    def status(self) -> dict[str, Any]: ...

    def verified_context(self, limit: int = 12) -> dict[str, Any]: ...


class FluctlightNative:
    """Direct Rust library call via PyO3 — same process as Python (like sqlite3)."""

    def __init__(self, brain_path: str, readonly: bool = True) -> None:
        import fluctlightdb_native as native  # type: ignore

        self._native = native
        if readonly:
            self._brain = native.Brain.open_readonly(brain_path)
        else:
            self._brain = native.Brain.open(brain_path)
        self.brain_path = brain_path

    def activate(
        self,
        cue: str,
        semantic_vector: Optional[list[float]] = None,
        agent_id: Optional[str] = None,
        limit: Optional[int] = None,
    ) -> dict[str, Any]:
        return self._brain.activate(cue, semantic_vector, agent_id, limit)

    def activate_batch(
        self,
        items: list[dict[str, Any]],
        limit: Optional[int] = None,
    ) -> dict[str, Any]:
        return self._brain.activate_batch_json(json.dumps(items), limit)

    def status(self) -> dict[str, Any]:
        return self._brain.status()

    def verified_context(self, limit: int = 12) -> dict[str, Any]:
        return self._brain.verified_context(limit)

    def has_sidecar_index(self) -> bool:
        return bool(self._brain.has_sidecar_index())


class FluctlightWorker:
    """Long-lived `fluctlight worker` subprocess — brain loaded once, sub-ms recall."""

    def __init__(
        self,
        brain_path: str,
        bin_path: Optional[str] = None,
    ) -> None:
        self.brain_path = brain_path
        self.bin_path = bin_path or _default_fluctlight_bin()
        self._lock = threading.Lock()
        self._id = 0
        self._proc: Optional[subprocess.Popen[str]] = None
        self._start()

    def _start(self) -> None:
        if not os.path.isfile(self.bin_path):
            raise FileNotFoundError(self.bin_path)
        self._proc = subprocess.Popen(
            [self.bin_path, "worker", "--path", self.brain_path],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        if self._proc.stdin is None or self._proc.stdout is None:
            raise RuntimeError("worker pipes unavailable")

    def close(self) -> None:
        with self._lock:
            if self._proc and self._proc.poll() is None:
                try:
                    self._call("shutdown")
                except Exception:
                    pass
                self._proc.terminate()
            self._proc = None

    def _call(self, op: str, **kwargs: Any) -> dict[str, Any]:
        with self._lock:
            if self._proc is None or self._proc.poll() is not None:
                self._start()
            assert self._proc is not None
            assert self._proc.stdin is not None
            assert self._proc.stdout is not None
            self._id += 1
            req = {"op": op, "id": self._id, **kwargs}
            self._proc.stdin.write(json.dumps(req) + "\n")
            self._proc.stdin.flush()
            line = self._proc.stdout.readline()
            if not line:
                err = (self._proc.stderr.read() if self._proc.stderr else "")[:300]
                raise RuntimeError(f"worker closed: {err}")
            resp = json.loads(line)
            if not resp.get("ok"):
                raise RuntimeError(resp.get("error") or "worker error")
            return resp

    def ping(self) -> bool:
        return bool(self._call("ping").get("pong"))

    def activate(
        self,
        cue: str,
        semantic_vector: Optional[list[float]] = None,
        agent_id: Optional[str] = None,
        limit: Optional[int] = None,
    ) -> dict[str, Any]:
        payload: dict[str, Any] = {"cue": cue}
        if semantic_vector is not None:
            payload["semantic_vector"] = semantic_vector
        if agent_id is not None:
            payload["agent_id"] = agent_id
        if limit is not None:
            payload["limit"] = limit
        return self._call("activate", **payload)["result"]

    def activate_batch(
        self,
        items: list[dict[str, Any]],
        limit: Optional[int] = None,
    ) -> dict[str, Any]:
        payload: dict[str, Any] = {"batch": items}
        if limit is not None:
            payload["limit"] = limit
        resp = self._call("activate_batch", **payload)
        return {"results": resp.get("results") or [], "count": resp.get("count", 0)}

    def status(self) -> dict[str, Any]:
        return self._call("status")["status"]

    def verified_context(self, limit: int = 12) -> dict[str, Any]:
        return self._call("verified_context", limit=limit)["context"]

    def reload(self) -> dict[str, Any]:
        return self._call("reload")


_worker_singleton: Optional[FluctlightWorker] = None
_native_singleton: Optional[FluctlightNative] = None
_client_lock = threading.Lock()


def get_recall_client(
    brain_path: Optional[str] = None,
    bin_path: Optional[str] = None,
) -> RecallClient:
    """Best available in-process recall: native library > worker subprocess."""
    global _native_singleton, _worker_singleton
    path = brain_path or os.environ.get(
        "FLUCTLIGHT_BRAIN",
        os.environ.get(
            "FLUCTLIGHT_BRAIN_PATH",
            os.path.expanduser("~/.fluctlight/tenants/default/brain"),
        ),
    )
    prefer_native = os.environ.get("FLUCTLIGHT_NATIVE", "1").lower() not in (
        "0",
        "false",
        "no",
    )
    with _client_lock:
        if prefer_native and _native_singleton is None:
            try:
                _native_singleton = FluctlightNative(path, readonly=True)
                return _native_singleton
            except ImportError:
                pass
            except Exception:
                _native_singleton = None
        if _native_singleton is not None:
            return _native_singleton
        if _worker_singleton is None:
            _worker_singleton = FluctlightWorker(path, bin_path=bin_path)
        return _worker_singleton


def get_worker(
    brain_path: Optional[str] = None,
    bin_path: Optional[str] = None,
) -> FluctlightWorker:
    client = get_recall_client(brain_path=brain_path, bin_path=bin_path)
    if isinstance(client, FluctlightWorker):
        return client
    raise TypeError("native client active — use get_recall_client() instead")
