"""Embedded brain client — sqlite3-style in-process API when native is installed."""

from __future__ import annotations

import json
import os
from typing import Any, Optional


class FluctlightBrain:
    """In-process Fluctlight brain (like ``sqlite3.connect``). Requires ``fluctlightdb-native``."""

    def __init__(self, brain: Any, *, readonly: bool = False) -> None:
        self._brain = brain
        self.readonly = readonly
        self.brain_path: Optional[str] = getattr(brain, "brain_path", None)

    @classmethod
    def connect(cls, path: str, *, readonly: bool = False) -> "FluctlightBrain":
        import fluctlightdb_native as native  # type: ignore

        brain = native.Brain.open_readonly(path) if readonly else native.Brain.open(path)
        obj = cls(brain, readonly=readonly)
        obj.brain_path = path
        return obj

    @classmethod
    def new(cls) -> "FluctlightBrain":
        import fluctlightdb_native as native  # type: ignore

        return cls(native.Brain.new(), readonly=False)

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
    """Open a brain directory like ``sqlite3.connect(path)``."""
    return FluctlightBrain.connect(path, readonly=readonly)
