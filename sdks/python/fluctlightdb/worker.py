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
        self.readonly = readonly

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

    def experience(self, episode_json: str) -> dict[str, Any]:
        if self.readonly:
            raise RuntimeError("brain opened readonly — reopen with readonly=False")
        return self._brain.experience(episode_json)

    def experience_dict(self, payload: dict[str, Any]) -> dict[str, Any]:
        return self.experience(json.dumps(payload))

    def verify_fact(
        self,
        engram_id: str,
        provenance_kind: str = "ledger_verified",
        source_uri: Optional[str] = None,
        confidence: float = 0.95,
    ) -> None:
        if self.readonly:
            raise RuntimeError("brain opened readonly")
        self._brain.verify_fact(engram_id, provenance_kind, source_uri, confidence)

    def sleep(self) -> dict[str, Any]:
        if self.readonly:
            raise RuntimeError("brain opened readonly")
        return self._brain.sleep()

    def tick(self, n: int = 1) -> list[dict[str, Any]]:
        if self.readonly:
            raise RuntimeError("brain opened readonly")
        return self._brain.tick(n)

    def preplay(self, goal: str, steps: int = 4) -> dict[str, Any]:
        return self._brain.preplay(goal, steps)

    def status(self) -> dict[str, Any]:
        return self._brain.status()

    def stage_report(self) -> dict[str, Any]:
        return self._brain.stage_report()

    def verified_context(self, limit: int = 12) -> dict[str, Any]:
        return self._brain.verified_context(limit)

    def has_sidecar_index(self) -> bool:
        return bool(self._brain.has_sidecar_index())

    def checkpoint(self) -> None:
        if self.readonly:
            raise RuntimeError("brain opened readonly")
        self._brain.checkpoint()


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
                    self._proc.stdin.write('{"op":"shutdown"}\n')
                    self._proc.stdin.flush()
                except Exception:
                    pass
                self._proc.terminate()
            self._proc = None

    def _call(self, op: str, **kwargs: Any) -> dict[str, Any]:
        with self._lock:
            if self._proc is None or self._proc.poll() is not None:
                self._start()
            assert self._proc and self._proc.stdin and self._proc.stdout
            self._id += 1
            req = {"id": self._id, "op": op, **kwargs}
            self._proc.stdin.write(json.dumps(req) + "\n")
            self._proc.stdin.flush()
            line = self._proc.stdout.readline()
            if not line:
                raise RuntimeError("worker closed stdout")
            resp = json.loads(line)
            if "error" in resp:
                raise RuntimeError(resp["error"])
            return resp.get("result", resp)

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
        return self._call("activate", **payload)

    def activate_batch(
        self,
        items: list[dict[str, Any]],
        limit: Optional[int] = None,
    ) -> dict[str, Any]:
        return self._call("activate_batch", batch=items, limit=limit)

    def status(self) -> dict[str, Any]:
        return self._call("status")

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
    *,
    readonly: bool = True,
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
                _native_singleton = FluctlightNative(path, readonly=readonly)
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
    client = get_recall_client(brain_path=brain_path, bin_path=bin_path, readonly=True)
    if isinstance(client, FluctlightWorker):
        return client
    raise TypeError("native client active — use get_recall_client() instead")
