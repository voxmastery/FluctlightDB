#!/usr/bin/env python3
"""Fluctlight embed sidecar — lazy all-MiniLM-L6-v2 on 127.0.0.1:8793"""

from __future__ import annotations

import logging
import os
from typing import List

from fastapi import FastAPI
from pydantic import BaseModel

LOG = logging.getLogger("fluctlight-embed")
app = FastAPI(title="Fluctlight Embed Sidecar")
_model = None


def get_model():
    global _model
    if _model is None:
        from sentence_transformers import SentenceTransformer

        name = os.environ.get("FLUCTLIGHT_EMBED_MODEL", "all-MiniLM-L6-v2")
        LOG.info("loading embed model %s", name)
        _model = SentenceTransformer(name)
    return _model


class EmbedRequest(BaseModel):
    text: str


class EmbedResponse(BaseModel):
    embedding: List[float]


@app.get("/health")
def health():
    return {"ok": True}


@app.post("/embed", response_model=EmbedResponse)
def embed(req: EmbedRequest):
    model = get_model()
    vec = model.encode(req.text, normalize_embeddings=True)
    return EmbedResponse(embedding=vec.tolist())


if __name__ == "__main__":
    import uvicorn

    logging.basicConfig(level=logging.INFO)
    host = os.environ.get("FLUCTLIGHT_EMBED_HOST", "127.0.0.1")
    port = int(os.environ.get("FLUCTLIGHT_EMBED_PORT", "8793"))
    uvicorn.run(app, host=host, port=port)
