"""FluctlightDB Python client — agent memory HTTP API."""

from __future__ import annotations

import http.client
import json
import os
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass, field
from typing import Any, Optional

from .brain import FluctlightBrain, connect
from .worker import FluctlightNative, FluctlightWorker, get_recall_client, get_worker

__all__ = [
    "FluctlightClient",
    "FluctlightBrain",
    "FluctlightNative",
    "FluctlightWorker",
    "connect",
    "get_recall_client",
    "get_worker",
]


@dataclass
class FluctlightClient:
    base_url: str = "http://127.0.0.1:8792"
    api_key: str = ""
    timeout: float = 60.0
    _http: Optional[http.client.HTTPConnection] = field(default=None, repr=False, compare=False)

    @classmethod
    def from_env(cls) -> "FluctlightClient":
        return cls(
            base_url=os.environ.get("FLUCTLIGHT_SERVE_URL", "http://127.0.0.1:8792").rstrip("/"),
            api_key=os.environ.get("FLUCTLIGHT_API_KEY", ""),
            timeout=float(os.environ.get("FLUCTLIGHT_HTTP_TIMEOUT", "60")),
        )

    def _headers(self) -> dict[str, str]:
        headers = {"Content-Type": "application/json", "Connection": "keep-alive"}
        if self.api_key:
            headers["Authorization"] = f"Bearer {self.api_key}"
        return headers

    def _conn(self) -> http.client.HTTPConnection:
        if self._http is not None:
            return self._http
        parsed = urllib.parse.urlparse(self.base_url)
        host = parsed.hostname or "127.0.0.1"
        port = parsed.port or (443 if parsed.scheme == "https" else 80)
        if parsed.scheme == "https":
            self._http = http.client.HTTPSConnection(host, port, timeout=self.timeout)
        else:
            self._http = http.client.HTTPConnection(host, port, timeout=self.timeout)
        return self._http

    def _post(self, path: str, payload: Optional[dict[str, Any]] = None) -> dict[str, Any]:
        body = json.dumps(payload or {})
        headers = self._headers()
        try:
            conn = self._conn()
            conn.request("POST", path, body=body, headers=headers)
            resp = conn.getresponse()
            raw = resp.read().decode("utf-8")
            if resp.status >= 400:
                raise RuntimeError(f"Fluctlight HTTP {resp.status}: {raw}")
            return json.loads(raw) if raw else {}
        except (http.client.HTTPException, OSError, RuntimeError):
            self._http = None
            url = f"{self.base_url}{path}"
            req = urllib.request.Request(
                url, data=body.encode("utf-8"), headers=self._headers(), method="POST"
            )
            try:
                with urllib.request.urlopen(req, timeout=self.timeout) as resp:
                    text = resp.read().decode("utf-8")
                    return json.loads(text) if text else {}
            except urllib.error.HTTPError as e:
                err_body = e.read().decode("utf-8", errors="replace")
                raise RuntimeError(f"Fluctlight HTTP {e.code}: {err_body}") from e

    def health(self) -> bool:
        req = urllib.request.Request(f"{self.base_url}/health", method="GET")
        try:
            with urllib.request.urlopen(req, timeout=min(self.timeout, 5.0)) as resp:
                return resp.status == 200
        except Exception:
            return False

    def status(self) -> dict[str, Any]:
        return self._post("/api/v1/status")

    def replica_status(self) -> dict[str, Any]:
        return self._post("/api/v1/replica-status")

    def experience(
        self,
        content: str,
        context: str = "api",
        salience: float = 0.5,
        outcome: Optional[str] = None,
        semantic_vector: Optional[list[float]] = None,
        agent_id: Optional[str] = None,
        doc_id: Optional[str] = None,
        chunk_id: Optional[str] = None,
        source_uri: Optional[str] = None,
        **extra: Any,
    ) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "content": content,
            "context": context,
            "salience": salience,
        }
        if outcome is not None:
            payload["outcome"] = outcome
        if semantic_vector is not None:
            payload["semantic_vector"] = semantic_vector
        if agent_id is not None:
            payload["agent_id"] = agent_id
        if doc_id is not None:
            payload["doc_id"] = doc_id
        if chunk_id is not None:
            payload["chunk_id"] = chunk_id
        if source_uri is not None:
            payload["source_uri"] = source_uri
        payload.update(extra)
        return self._post("/api/v1/experience", payload)

    def ingest_chunk(
        self,
        content: str,
        doc_id: str,
        chunk_id: str,
        source_uri: Optional[str] = None,
        semantic_vector: Optional[list[float]] = None,
        salience: float = 0.55,
        agent_id: Optional[str] = None,
        outcome: Optional[str] = None,
    ) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "content": content,
            "doc_id": doc_id,
            "chunk_id": chunk_id,
            "salience": salience,
        }
        if source_uri:
            payload["source_uri"] = source_uri
        if semantic_vector is not None:
            payload["semantic_vector"] = semantic_vector
        if agent_id:
            payload["agent_id"] = agent_id
        if outcome:
            payload["outcome"] = outcome
        return self._post("/api/v1/ingest-chunk", payload)

    def activate(
        self,
        cue: str,
        semantic_vector: Optional[list[float]] = None,
        agent_id: Optional[str] = None,
    ) -> dict[str, Any]:
        payload: dict[str, Any] = {"cue": cue}
        if semantic_vector is not None:
            payload["semantic_vector"] = semantic_vector
        if agent_id is not None:
            payload["agent_id"] = agent_id
        return self._post("/api/v1/activate", payload)

    def activate_lite(
        self,
        cue: str,
        semantic_vector: Optional[list[float]] = None,
        agent_id: Optional[str] = None,
    ) -> dict[str, Any]:
        """Top-1 recall — minimal JSON (~200 B) for HTTP agent hot paths."""
        payload: dict[str, Any] = {"cue": cue, "limit": 1}
        if semantic_vector is not None:
            payload["semantic_vector"] = semantic_vector
        if agent_id is not None:
            payload["agent_id"] = agent_id
        return self._post("/api/v1/activate-lite", payload)

    def activate_batch(
        self,
        items: list[dict[str, Any]],
    ) -> dict[str, Any]:
        return self._post("/api/v1/activate-batch", {"batch": items})

    def query(self, op: dict[str, Any]) -> dict[str, Any]:
        return self._post("/api/v1/query", {"query": op})

    def search_by_rag(
        self,
        doc_id: str,
        chunk_id: Optional[str] = None,
        page: int = 0,
        page_size: int = 50,
    ) -> dict[str, Any]:
        q: dict[str, Any] = {
            "op": "search_by_rag",
            "doc_id": doc_id,
            "page": page,
            "page_size": page_size,
        }
        if chunk_id is not None:
            q["chunk_id"] = chunk_id
        return self.query(q)

    def tick(self, n: int = 1) -> Any:
        return self._post("/api/v1/tick", {"n": int(n)})

    def compact(self) -> dict[str, Any]:
        return self._post("/api/v1/compact")

    def export_viz(self) -> dict[str, Any]:
        return self._post("/api/v1/export-viz")

    def export_graph(self, *, lite: bool = False) -> dict[str, Any]:
        path = "/api/v1/export-graph-lite" if lite else "/api/v1/export-graph"
        return self._post(path)

    def export_raw(self) -> dict[str, Any]:
        return self._post("/api/v1/export-raw")

    def reward(self, magnitude: float = 0.5) -> dict[str, Any]:
        return self._post("/api/v1/reward", {"magnitude": float(magnitude)})

    def death(self, cause: str = "api") -> dict[str, Any]:
        return self._post("/api/v1/death", {"cause": cause[:200]})

    def mark_core(self, engram_id: str, key: str = "core") -> dict[str, Any]:
        return self._post("/api/v1/mark-core", {"engram_id": str(engram_id), "key": key})

    def verify_fact(
        self,
        engram_id: str,
        *,
        provenance_kind: str = "ledger_verified",
        source_uri: Optional[str] = None,
        confidence: float = 0.95,
    ) -> dict[str, Any]:
        payload: dict[str, Any] = {
            "engram_id": str(engram_id),
            "provenance_kind": provenance_kind,
            "confidence": confidence,
        }
        if source_uri:
            payload["source_uri"] = source_uri
        return self._post("/api/v1/verify-fact", payload)

    def ground_wallet(
        self,
        balance: float,
        level: int = 1,
        wallet_path: Optional[str] = None,
    ) -> dict[str, Any]:
        uri = wallet_path or "file://wallet.json"
        content = (
            f"ledger verified: agent wallet balance is ${balance:.2f} at level {level} "
            f"(source: wallet.json — ground truth, not chat memory)"
        )
        rep = self.experience(
            content[:500],
            context="ledger:wallet",
            salience=0.98,
            doc_id="ledger",
            chunk_id="wallet-balance",
            source_uri=uri,
            verified=True,
            provenance_kind="ledger_verified",
            confidence=0.99,
        )
        eid = rep.get("engram_id")
        if eid and not rep.get("deduplicated"):
            self.mark_core(str(eid), "ledger-wallet-balance")
        return rep

    def preplay(self, goal: str, steps: int = 4) -> dict[str, Any]:
        return self._post("/api/v1/preplay", {"goal": goal, "steps": int(steps)})

    def neurogenesis(self) -> dict[str, Any]:
        return self._post("/api/v1/neurogenesis", {})

    def verified_context(self, limit: int = 12) -> dict[str, Any]:
        return self._post("/api/v1/verified-context", {"limit": int(limit)})

    def stage_report(self) -> dict[str, Any]:
        return self._post("/api/v1/stage-report", {})
