"""Embedded brain client — sqlite3-style in-process API when native is installed."""

from __future__ import annotations

import json
import os
from typing import Any, Optional


def _require_native() -> Any:
    """Import the native extension or raise an actionable install hint."""
    try:
        import fluctlightdb_native as native  # type: ignore

        return native
    except ImportError as exc:
        raise ImportError(
            "Embedded mode needs the native extension, which isn't installed.\n"
            "  Install it with:   pip install 'fluctlightdb[native]'\n"
            "  Or build locally:  pip install fluctlightdb-native\n"
            "If no prebuilt wheel exists for your platform, a Rust toolchain "
            "(https://rustup.rs) is required to build from source.\n"
            "For the pure-Python HTTP client (no native build), use "
            "FluctlightClient instead of connect()."
        ) from exc


class FluctlightBrain:
    """In-process Fluctlight brain (like ``sqlite3.connect``). Requires ``fluctlightdb-native``."""

    MODE_AGENT = "agent"
    MODE_INDEX = "index"
    MODE_CONV = "conv"

    def __init__(self, brain: Any, *, readonly: bool = False, mode: str = MODE_AGENT) -> None:
        self._brain = brain
        self.readonly = readonly
        self._mode = mode
        self.brain_path: Optional[str] = getattr(brain, "brain_path", None)
        if mode == self.MODE_INDEX:
            self._enable_index_mode()
        elif mode == self.MODE_CONV:
            self._enable_conv_mode()

    @staticmethod
    def _enable_index_mode() -> None:
        """Bulk IR path: fast ingest + vector-fast recall (Chroma-class speed)."""
        os.environ["FLUCTLIGHT_FAST_INGEST"] = "1"
        os.environ["FLUCTLIGHT_VECTOR_FAST"] = "1"
        os.environ.setdefault("FLUCTLIGHT_CANDIDATE_CAP", "512")

    @staticmethod
    def _enable_agent_mode() -> None:
        os.environ.pop("FLUCTLIGHT_FAST_INGEST", None)
        os.environ.pop("FLUCTLIGHT_VECTOR_FAST", None)

    @staticmethod
    def _enable_conv_mode() -> None:
        """Conversational RAG: fast bulk ingest + hybrid recall (LoCoMo / LongMemEval)."""
        os.environ["FLUCTLIGHT_FAST_INGEST"] = "1"
        os.environ.pop("FLUCTLIGHT_VECTOR_FAST", None)
        os.environ.setdefault("FLUCTLIGHT_CANDIDATE_CAP", "512")

    @property
    def mode(self) -> str:
        return self._mode

    @classmethod
    def connect(cls, path: str, *, readonly: bool = False) -> "FluctlightBrain":
        cls._enable_agent_mode()
        native = _require_native()
        brain = native.Brain.open_readonly(path) if readonly else native.Brain.open(path)
        obj = cls(brain, readonly=readonly, mode=cls.MODE_AGENT)
        obj.brain_path = path
        return obj

    @classmethod
    def connect_index(cls, path: Optional[str] = None, *, readonly: bool = False) -> "FluctlightBrain":
        """Open a brain tuned for bulk semantic indexing (fast write + vector recall).

        Use for RAG backfills and IR benchmarks. For live agent episodic memory,
        use ``connect()`` instead (full dentate separation, graph, provenance).
        """
        # Must set env before native import so fast paths apply on first Brain.new().
        cls._enable_index_mode()
        native = _require_native()
        if path:
            brain = native.Brain.open_readonly(path) if readonly else native.Brain.open(path)
            obj = cls(brain, readonly=readonly, mode=cls.MODE_INDEX)
            obj.brain_path = path
            return obj
        return cls(native.Brain.new(), readonly=False, mode=cls.MODE_INDEX)

    @classmethod
    def connect_conv(cls, path: Optional[str] = None, *, readonly: bool = False) -> "FluctlightBrain":
        """Conversational memory / RAG benchmarks: fast ingest + hybrid lexical+semantic recall."""
        cls._enable_conv_mode()
        native = _require_native()
        if path:
            brain = native.Brain.open_readonly(path) if readonly else native.Brain.open(path)
            obj = cls(brain, readonly=readonly, mode=cls.MODE_CONV)
            obj.brain_path = path
            return obj
        return cls(native.Brain.new(), readonly=False, mode=cls.MODE_CONV)

    @classmethod
    def new(cls) -> "FluctlightBrain":
        native = _require_native()
        cls._enable_agent_mode()
        return cls(native.Brain.new(), readonly=False, mode=cls.MODE_AGENT)

    def experience(
        self,
        content: str,
        *,
        context: str = "api",
        salience: float = 0.5,
        outcome: Optional[str] = None,
        semantic_vector: Optional[list[float]] = None,
        agent_id: Optional[str] = None,
        tenant_id: Optional[str] = None,
        verified: Optional[bool] = None,
        provenance_kind: Optional[str] = None,
        source_uri: Optional[str] = None,
        confidence: Optional[float] = None,
        doc_id: Optional[str] = None,
        chunk_id: Optional[str] = None,
        **extra: Any,
    ) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "content": content,
            "context": context,
            "salience_hint": salience,
        }
        if outcome is not None:
            payload["outcome"] = outcome
        if semantic_vector is not None:
            payload["semantic_vector"] = semantic_vector
        if agent_id is not None:
            payload["agent_id"] = agent_id
        if tenant_id is not None:
            payload["tenant_id"] = tenant_id
        if doc_id or chunk_id or source_uri:
            payload["rag"] = {
                "doc_id": doc_id,
                "chunk_id": chunk_id,
                "source_uri": source_uri,
            }
        if verified is not None or provenance_kind or source_uri:
            payload["provenance"] = {
                "kind": provenance_kind or "ledger_verified",
                "source_uri": source_uri,
                "confidence": confidence if confidence is not None else 0.95,
                "verified": bool(verified),
            }
        payload.update(extra)
        return self._brain.experience(json.dumps(payload))

    def activate(
        self,
        cue: str,
        semantic_vector: Optional[list[float]] = None,
        agent_id: Optional[str] = None,
        limit: Optional[int] = None,
    ) -> dict[str, Any]:
        """Recall by cue. Returns a dict shaped like::

            {
              "recalls": [
                {"engram_id": str, "activation": float, "verified": bool,
                 "trust_note": str | None,
                 "episode": {"content": str, "context": str, ...}},
                ...
              ],
              "active_neurons": int, "hops": int, "myelinated": bool,
            }

        Pass ``semantic_vector`` (your own embedding) to add semantic recall on
        top of lexical/spreading-activation matching.
        """
        return self._brain.activate(cue, semantic_vector, agent_id, limit)

    def activate_batch(
        self,
        items: list[dict[str, Any]],
        limit: Optional[int] = None,
    ) -> dict[str, Any]:
        return self._brain.activate_batch_json(json.dumps(items), limit)

    def verify_fact(
        self,
        engram_id: str,
        *,
        provenance_kind: str = "ledger_verified",
        source_uri: Optional[str] = None,
        confidence: float = 0.95,
    ) -> None:
        self._brain.verify_fact(engram_id, provenance_kind, source_uri, confidence)

    def sleep(self) -> dict[str, Any]:
        return self._brain.sleep()

    def tick(self, n: int = 1) -> list[dict[str, Any]]:
        return self._brain.tick(n)

    def preplay(self, goal: str, steps: int = 4) -> dict[str, Any]:
        return self._brain.preplay(goal, steps)

    def neurogenesis(self) -> dict[str, Any]:
        return self._brain.neurogenesis_pulse()

    def compact(self) -> dict[str, Any]:
        return self._brain.compact()

    def reward(self, magnitude: float = 0.5) -> None:
        self._brain.reward(magnitude)

    def mark_core(self, engram_id: str, key: str) -> None:
        self._brain.mark_core(engram_id, key)

    def death(self, cause: str = "api") -> str:
        return str(self._brain.death(cause))

    def status(self) -> dict[str, Any]:
        return self._brain.status()

    def stage_report(self) -> dict[str, Any]:
        return self._brain.stage_report()

    def verified_context(self, limit: int = 12) -> dict[str, Any]:
        return self._brain.verified_context(limit)

    def stage(self) -> str:
        return str(self._brain.stage())

    def checkpoint(self) -> None:
        self._brain.checkpoint()

    def has_sidecar_index(self) -> bool:
        return bool(self._brain.has_sidecar_index())


def connect(path: str, *, readonly: bool = False) -> FluctlightBrain:
    """Open an agent brain directory (full episodic memory path)."""
    return FluctlightBrain.connect(path, readonly=readonly)


def connect_index(path: Optional[str] = None, *, readonly: bool = False) -> FluctlightBrain:
    """Open a bulk semantic index (fast ingest + vector recall)."""
    return FluctlightBrain.connect_index(path, readonly=readonly)


def connect_conv(path: Optional[str] = None, *, readonly: bool = False) -> FluctlightBrain:
    """Conversational RAG mode: fast ingest + hybrid recall."""
    return FluctlightBrain.connect_conv(path, readonly=readonly)
