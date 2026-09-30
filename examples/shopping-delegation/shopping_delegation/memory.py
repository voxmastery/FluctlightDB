"""Memory backends. The agent policy is not in here.

FluctlightMemory uses the public embedded API: ``connect_embedded``,
``experience`` (with provenance), ``activate``, and ``checkpoint``.
Chat-log and TF-IDF are local baselines with no extra packages.
"""

from __future__ import annotations

import gc
import math
import os
import re
import time
from dataclasses import dataclass
from typing import Any, Optional, Protocol

from .policy import TOP_K

_TOKEN = re.compile(r"[a-z0-9_]+", re.IGNORECASE)


def tokenize(text: str) -> list[str]:
    return [m.group(0).lower() for m in _TOKEN.finditer(text)]


@dataclass
class Hit:
    text: str
    score: float
    verified: Optional[bool] = None
    provenance_kind: Optional[str] = None
    source_uri: Optional[str] = None
    engram_id: Optional[str] = None
    doc_id: Optional[str] = None
    backend: str = ""

    def as_dict(self) -> dict[str, Any]:
        return {
            "text": self.text,
            "score": self.score,
            "verified": self.verified,
            "provenance_kind": self.provenance_kind,
            "source_uri": self.source_uri,
            "engram_id": self.engram_id,
            "doc_id": self.doc_id,
            "backend": self.backend,
        }


class Memory(Protocol):
    name: str

    def add(self, text: str, *, meta: dict[str, Any]) -> dict[str, Any]:
        ...

    def recall(self, cue: str, limit: int = TOP_K) -> tuple[list[Hit], float]:
        """Return hits and elapsed milliseconds."""

    def close(self) -> None:
        ...


def _rank(scored: list[tuple[float, int, str]], limit: int) -> list[Hit]:
    # Higher score first. Tie-break is the second element (caller-defined).
    scored.sort(key=lambda row: (-row[0], row[1]))
    hits: list[Hit] = []
    for score, _tie, text in scored[:limit]:
        hits.append(Hit(text=text, score=score))
    return hits


class ChatLogMemory:
    """Full transcript. Score is the fraction of cue tokens found in the line.

    Equal scores keep the later line (recency tie-break only). This is the
    chat-log baseline: no provenance, no vectors.
    """

    name = "chat_log"

    def __init__(self) -> None:
        self._docs: list[str] = []

    def add(self, text: str, *, meta: dict[str, Any]) -> dict[str, Any]:
        del meta
        self._docs.append(text)
        return {"stored": True}

    def recall(self, cue: str, limit: int = TOP_K) -> tuple[list[Hit], float]:
        t0 = time.perf_counter()
        q = tokenize(cue)
        q_set = set(q)
        scored: list[tuple[float, int, str]] = []
        if q_set:
            for index, doc in enumerate(self._docs):
                d_set = set(tokenize(doc))
                overlap = len(q_set & d_set) / len(q_set)
                if overlap <= 0:
                    continue
                # Negative index so a later row wins a tie when we sort ascending.
                scored.append((overlap, -index, doc))
        hits = _rank(scored, limit)
        for hit in hits:
            hit.backend = self.name
        elapsed = (time.perf_counter() - t0) * 1000.0
        return hits, elapsed

    def close(self) -> None:
        self._docs.clear()


class TfidfMemory:
    """In-process TF-IDF cosine. Standard library only.

    Tie-break keeps the earlier row, which is the naive behaviour when cosine
    does not encode recency.
    """

    name = "tfidf"

    def __init__(self) -> None:
        self._docs: list[list[str]] = []
        self._raw: list[str] = []

    def add(self, text: str, *, meta: dict[str, Any]) -> dict[str, Any]:
        del meta
        self._raw.append(text)
        self._docs.append(tokenize(text))
        return {"stored": True}

    def recall(self, cue: str, limit: int = TOP_K) -> tuple[list[Hit], float]:
        t0 = time.perf_counter()
        hits = self._search(cue, limit)
        for hit in hits:
            hit.backend = self.name
        elapsed = (time.perf_counter() - t0) * 1000.0
        return hits, elapsed

    def _search(self, cue: str, limit: int) -> list[Hit]:
        query = tokenize(cue)
        if not query or not self._docs:
            return []
        n_docs = len(self._docs)
        df: dict[str, int] = {}
        for doc in self._docs:
            for tok in set(doc):
                df[tok] = df.get(tok, 0) + 1
        idf = {
            tok: math.log((1.0 + n_docs) / (1.0 + freq)) + 1.0 for tok, freq in df.items()
        }
        q_tf: dict[str, float] = {}
        for tok in query:
            q_tf[tok] = q_tf.get(tok, 0.0) + 1.0
        q_vec = {tok: (count / len(query)) * idf.get(tok, 0.0) for tok, count in q_tf.items()}
        q_norm = math.sqrt(sum(v * v for v in q_vec.values()))
        if q_norm == 0.0:
            return []
        scored: list[tuple[float, int, str]] = []
        for index, doc in enumerate(self._docs):
            if not doc:
                continue
            tf: dict[str, float] = {}
            for tok in doc:
                tf[tok] = tf.get(tok, 0.0) + 1.0
            dot = 0.0
            d_norm_sq = 0.0
            for tok, count in tf.items():
                weight = (count / len(doc)) * idf.get(tok, 0.0)
                d_norm_sq += weight * weight
                if tok in q_vec:
                    dot += weight * q_vec[tok]
            if d_norm_sq <= 0.0 or dot <= 0.0:
                continue
            cosine = dot / (q_norm * math.sqrt(d_norm_sq))
            scored.append((cosine, index, self._raw[index]))
        return _rank(scored, limit)

    def close(self) -> None:
        self._docs.clear()
        self._raw.clear()


class FluctlightMemory:
    """Embedded FluctlightDB brain.

    Provenance is whatever ``experience`` / ``activate`` actually store and
    return: ``verified``, ``provenance.kind``, ``provenance.source_uri``,
    ``provenance.confidence``. There is no separate delegation graph.
    Retention is left unlimited (``retain_days=None``) so a short synthetic
    session is not pruned mid-benchmark.
    """

    name = "fluctlight"

    def __init__(self, path: str) -> None:
        from fluctlightdb import connect_embedded

        self.path = path
        self.brain = connect_embedded(path, retain_days=None)
        self.stored = 0
        self.rejected = 0
        self.deduplicated = 0
        self.chat_rejected = 0

    def add(self, text: str, *, meta: dict[str, Any]) -> dict[str, Any]:
        kind = str(meta.get("provenance_kind") or "chat_assertion")
        verified = bool(meta.get("verified", False))
        report = self.brain.experience(
            text,
            context=str(meta.get("context") or "shopping"),
            salience=float(meta.get("salience", 0.55)),
            verified=verified,
            provenance_kind=kind,
            source_uri=meta.get("source_uri"),
            confidence=float(meta.get("confidence", 0.99 if verified else 0.25)),
            doc_id=meta.get("doc_id"),
            chunk_id=meta.get("chunk_id"),
        )
        rejected = bool(report.get("gate_rejected"))
        dedup = bool(report.get("deduplicated"))
        if rejected:
            self.rejected += 1
            if not verified:
                self.chat_rejected += 1
        elif dedup:
            self.deduplicated += 1
        else:
            self.stored += 1
        return {
            "stored": not rejected and not dedup,
            "gate_rejected": rejected,
            "deduplicated": dedup,
            "engram_id": str(report.get("engram_id") or ""),
            "gate_reason": report.get("gate_reason"),
        }

    def recall(self, cue: str, limit: int = TOP_K) -> tuple[list[Hit], float]:
        t0 = time.perf_counter()
        raw = self.brain.activate(cue, limit=limit)
        elapsed = (time.perf_counter() - t0) * 1000.0
        recalls = raw.get("recalls") if isinstance(raw, dict) else None
        hits: list[Hit] = []
        for row in recalls or []:
            episode = row.get("episode") or {}
            provenance = episode.get("provenance") or {}
            rag = episode.get("rag") or {}
            verified = row.get("verified")
            if verified is None:
                verified = provenance.get("verified")
            hits.append(
                Hit(
                    text=str(episode.get("content") or ""),
                    score=float(row.get("activation") or 0.0),
                    verified=None if verified is None else bool(verified),
                    provenance_kind=provenance.get("kind"),
                    source_uri=provenance.get("source_uri"),
                    engram_id=str(row.get("engram_id") or "") or None,
                    doc_id=rag.get("doc_id"),
                    backend=self.name,
                )
            )
        return hits, elapsed

    def checkpoint(self) -> None:
        self.brain.checkpoint()

    def close(self) -> None:
        brain = self.brain
        self.brain = None
        del brain
        gc.collect()


def meta_for_event(event: dict[str, Any], *, session_id: str, seq: int) -> dict[str, Any]:
    role = str(event.get("role") or "chat")
    record = event.get("record") or {}
    record_id = str(record.get("record_id") or f"{session_id}-chat-{seq}")
    item = str(record.get("item") or "chat")
    if role == "delegation":
        return {
            "provenance_kind": "user_explicit",
            "verified": True,
            "confidence": 0.99,
            "salience": 0.95,
            "source_uri": record.get("source_uri"),
            "context": f"delegation:{item}",
            "doc_id": record_id,
            "chunk_id": f"{session_id}:{record_id}",
        }
    if role == "revocation":
        return {
            "provenance_kind": "ledger_verified",
            "verified": True,
            "confidence": 0.99,
            "salience": 0.97,
            "source_uri": record.get("source_uri"),
            "context": f"revocation:{item}",
            "doc_id": record_id,
            "chunk_id": f"{session_id}:{record_id}",
        }
    if role == "purchase":
        return {
            "provenance_kind": "tool_grounded",
            "verified": True,
            "confidence": 0.93,
            "salience": 0.8,
            "source_uri": record.get("source_uri"),
            "context": f"purchase:{item}",
            "doc_id": record_id,
            "chunk_id": f"{session_id}:{record_id}",
        }
    return {
        "provenance_kind": "chat_assertion",
        "verified": False,
        "confidence": 0.22,
        "salience": 0.4,
        "source_uri": None,
        "context": f"chat:{item}",
        "doc_id": record_id,
        "chunk_id": f"{session_id}:{record_id}",
    }


def fresh_memory(name: str, directory: str) -> Memory:
    if name == "chat_log":
        return ChatLogMemory()
    if name == "tfidf":
        return TfidfMemory()
    if name == "fluctlight":
        path = os.path.join(directory, "brain")
        return FluctlightMemory(path)
    raise ValueError(name)
