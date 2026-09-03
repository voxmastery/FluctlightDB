"""Fluctlight rerank sidecar — CPU cross-encoder to fix the recall rank-cutoff.
Re-scores brain recall candidates by true query-relevance so the right memory
surfaces above the injection window. Lazy-loads ms-marco-MiniLM-L-6-v2 (~90MB, CPU).
Fail-safe by design: the caller falls back to original order if this is down."""
import logging, os
from fastapi import FastAPI
from pydantic import BaseModel

logging.basicConfig(level=logging.INFO)
LOG = logging.getLogger("fluctlight-rerank")
app = FastAPI(title="Fluctlight Rerank Sidecar")
_model = None

def get_model():
    global _model
    if _model is None:
        from sentence_transformers import CrossEncoder
        name = os.environ.get("FLUCTLIGHT_RERANK_MODEL", "cross-encoder/ms-marco-MiniLM-L-6-v2")
        LOG.info("loading reranker %s", name)
        _model = CrossEncoder(name, max_length=512)
    return _model

class RerankReq(BaseModel):
    query: str
    candidates: list[str]
    top_k: int | None = None

@app.get("/health")
def health():
    return {"ok": True, "loaded": _model is not None}

@app.post("/rerank")
def rerank(req: RerankReq):
    if not req.candidates:
        return {"order": [], "scores": []}
    m = get_model()
    pairs = [(req.query, c) for c in req.candidates]
    scores = m.predict(pairs, convert_to_numpy=True).tolist()
    order = sorted(range(len(scores)), key=lambda i: scores[i], reverse=True)
    if req.top_k:
        order = order[:req.top_k]
    return {"order": order, "scores": [scores[i] for i in order]}
