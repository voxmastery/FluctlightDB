#!/usr/bin/env python3
"""Shared lane connectors for certified benchmarks (Muon / CHORUS / agent)."""

from __future__ import annotations

import os
from typing import Any, Optional

from fluctlightdb.brain import FluctlightBrain


def embed_minilm(texts: list[str]) -> list[list[float]]:
    from chromadb.utils import embedding_functions

    emb = embedding_functions.ONNXMiniLM_L6_V2()
    return [list(map(float, v)) for v in emb(texts)]


def open_lane(mode: str, path: Optional[str] = None) -> Any:
    """Return a brain for the requested modern lane."""
    mode = mode.lower()
    if mode == "chorus":
        return FluctlightBrain.connect_chorus(path) if path else FluctlightBrain.connect_chorus()
    if mode == "muon":
        return FluctlightBrain.connect_muon(path) if path else FluctlightBrain.connect_muon()
    if mode == "agent":
        return FluctlightBrain.connect_agent(path) if path else FluctlightBrain.connect_agent()
    if mode == "brain":
        return FluctlightBrain.connect_brain(path) if path else FluctlightBrain.connect_brain()
    raise ValueError(f"unknown lane: {mode}")


def chorus_imprint_rows(
    brain: Any,
    rows: list[dict[str, Any]],
    *,
    id_key: str = "memory_id",
    content_key: str = "content",
    context_key: str = "context",
    vector_key: str = "semantic_vector",
) -> int:
    batch = [
        {
            "memory_id": str(r[id_key]),
            "content": str(r[content_key]),
            "context": str(r.get(context_key) or r[id_key]),
            "semantic_vector": r.get(vector_key),
            "salience": float(r.get("salience", 0.6)),
        }
        for r in rows
    ]
    return int(brain.chorus_imprint_batch(batch))


def chorus_hits_to_ids(hits: list[Any], limit: int) -> list[str]:
    ids: list[str] = []
    for h in hits[:limit]:
        if isinstance(h, (list, tuple)) and h:
            ids.append(str(h[0]))
        elif isinstance(h, dict):
            mid = h.get("memory_id")
            if mid:
                ids.append(str(mid))
    return ids


def configure_paper_fabric() -> None:
    """Paper-profile benchmarks: Recall Fabric on for all frozen headline numbers."""
    os.environ["FLUCTLIGHT_FABRIC"] = "1"


def configure_ir_env() -> None:
    """Tune env for bulk IR benchmarks (BEIR / LoCoMo CHORUS)."""
    configure_paper_fabric()
    os.environ.setdefault("FLUCTLIGHT_CHECKPOINT_EVERY_N", "100000")
    os.environ.setdefault("FLUCTLIGHT_WAL", "0")
    os.environ.setdefault("FLUCTLIGHT_SEPARATION_GATE", "0")
