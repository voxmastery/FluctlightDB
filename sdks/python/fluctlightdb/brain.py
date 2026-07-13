"""Embedded brain client — sqlite3-style in-process API when native is installed."""

from __future__ import annotations

import json
import os
from typing import Any, Optional


def _secure_brain_directory(path: str) -> None:
    """Best-effort 0700 on brain parent/dir (Unix only)."""
    if os.name == "nt":
        return
    abs_path = os.path.abspath(path)
    parent = os.path.dirname(abs_path)
    if parent:
        os.makedirs(parent, mode=0o700, exist_ok=True)
        try:
            os.chmod(parent, 0o700)
        except OSError:
            pass
    if os.path.isdir(abs_path):
        try:
            os.chmod(abs_path, 0o700)
        except OSError:
            pass


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
    MODE_AGENT_FAST = "agent_fast"
    MODE_BRAIN = "brain"
    MODE_INDEX = "index"
    MODE_CONV = "conv"
    MODE_CHORUS = "chorus"
    MODE_AGENT_UNIFIED = "agent_unified"

    def __init__(self, brain: Any, *, readonly: bool = False, mode: str = MODE_AGENT) -> None:
        self._brain = brain
        self.readonly = readonly
        self._mode = mode
        self.brain_path: Optional[str] = getattr(brain, "brain_path", None)
        if mode == self.MODE_INDEX:
            self._enable_index_mode()
        elif mode == self.MODE_CONV:
            self._enable_conv_mode()
        elif mode == self.MODE_AGENT_FAST:
            self._enable_agent_fast_mode()
        elif mode == self.MODE_BRAIN:
            self._enable_brain_mode()

    @staticmethod
    def _enable_brain_mode() -> None:
        """Brain benchmark + agent recall: hybrid ingest, graph spread, cortex boost, CLS sleep."""
        os.environ["FLUCTLIGHT_FAST_INGEST"] = "1"
        os.environ.pop("FLUCTLIGHT_VECTOR_FAST", None)
        os.environ.pop("FLUCTLIGHT_AGENT_FAST", None)
        os.environ.setdefault("FLUCTLIGHT_CANDIDATE_CAP", "512")
        os.environ.setdefault("FLUCTLIGHT_CORTEX_WEIGHT", "0.35")

    @staticmethod
    def _enable_muon_mode() -> None:
        """Muon + Tau Lane: penetrative bulk imprint + episodic fission (0 embed HTTP)."""
        os.environ["FLUCTLIGHT_MUON"] = "1"
        os.environ["FLUCTLIGHT_TAU"] = "1"
        os.environ["FLUCTLIGHT_FAST_INGEST"] = "1"
        os.environ.pop("FLUCTLIGHT_VECTOR_FAST", None)

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
        os.environ.pop("FLUCTLIGHT_AGENT_FAST", None)

    @staticmethod
    def _enable_agent_fast_mode() -> None:
        """Live agent recall: hybrid sidecar pre-filter + 1-hop spread (full write path)."""
        os.environ.pop("FLUCTLIGHT_FAST_INGEST", None)
        os.environ.pop("FLUCTLIGHT_VECTOR_FAST", None)
        os.environ["FLUCTLIGHT_AGENT_FAST"] = "1"
        os.environ.setdefault("FLUCTLIGHT_CANDIDATE_CAP", "96")

    @staticmethod
    def _enable_agent_unified_mode() -> None:
        """One connection — fast episodic + CHORUS corpus + auto-consolidate + WM-Ring."""
        os.environ["FLUCTLIGHT_AGENT_ERGONOMICS"] = "1"
        os.environ["FLUCTLIGHT_CHORUS"] = "1"
        os.environ["FLUCTLIGHT_CHORUS_FAST"] = "1"
        os.environ.setdefault("FLUCTLIGHT_CHORUS_FLOAT_RERANK", "1")
        os.environ["FLUCTLIGHT_FAST_INGEST"] = "1"
        os.environ.pop("FLUCTLIGHT_VECTOR_FAST", None)
        os.environ["FLUCTLIGHT_AGENT_FAST"] = "1"
        os.environ.setdefault("FLUCTLIGHT_CANDIDATE_CAP", "512")

    @classmethod
    def connect_agent(
        cls,
        path: Optional[str] = None,
        *,
        readonly: bool = False,
        retain_days: Optional[int] = 30,
    ) -> "FluctlightBrain":
        """Recommended agent entry point — unified recall, WM-Ring, auto-consolidate.

        Replaces picking between connect / connect_agent_fast / connect_chorus manually.
        """
        cls._enable_agent_unified_mode()
        native = _require_native()
        if path:
            brain = native.Brain.open_readonly(path) if readonly else native.Brain.open(path)
            obj = cls(brain, readonly=readonly, mode=cls.MODE_AGENT_UNIFIED)
            obj.brain_path = path
        else:
            obj = cls(native.Brain.new(), readonly=False, mode=cls.MODE_AGENT_UNIFIED)
        if retain_days is not None and not readonly:
            obj.retain_for(days=retain_days)
            obj.set_auto_consolidate(True)
        return obj

    @classmethod
    def connect_embedded(
        cls,
        path: str,
        *,
        readonly: bool = False,
        retain_days: Optional[int] = 30,
        secure_dir: bool = True,
    ) -> "FluctlightBrain":
        """Production embedded entry — in-process brain, safe env defaults, no HTTP serve flags.

        Same unified recall / WM-Ring as :meth:`connect_agent`, but clears serve/auth env pollution
        and optionally chmods the brain directory to ``0700`` on Unix.
        """
        for key in (
            "FLUCTLIGHT_VECTOR_FAST",
            "FLUCTLIGHT_REQUIRE_AUTH",
            "FLUCTLIGHT_API_KEYS",
            "FLUCTLIGHT_SERVE_URL",
            "FLUCTLIGHT_API_KEY",
            "FLUCTLIGHT_HTTP_TIMEOUT",
        ):
            os.environ.pop(key, None)
        cls._enable_agent_unified_mode()
        if secure_dir and not readonly:
            _secure_brain_directory(path)
        native = _require_native()
        brain = native.Brain.open_readonly(path) if readonly else native.Brain.open(path)
        obj = cls(brain, readonly=readonly, mode=cls.MODE_AGENT_UNIFIED)
        obj.brain_path = path
        if retain_days is not None and not readonly:
            obj.retain_for(days=retain_days)
            obj.set_auto_consolidate(True)
        return obj

    @staticmethod
    def _enable_chorus_mode() -> None:
        """CHORUS phase field: wavelet imprint + GRG fast recall."""
        os.environ["FLUCTLIGHT_CHORUS"] = "1"
        os.environ["FLUCTLIGHT_CHORUS_FAST"] = "1"
        os.environ.setdefault("FLUCTLIGHT_CHORUS_FLOAT_RERANK", "1")
        os.environ["FLUCTLIGHT_FAST_INGEST"] = "1"
        os.environ["FLUCTLIGHT_VECTOR_FAST"] = "1"
        os.environ.setdefault("FLUCTLIGHT_CANDIDATE_CAP", "512")

    @classmethod
    def connect_chorus(cls, path: Optional[str] = None, *, readonly: bool = False) -> "FluctlightBrain":
        """CHORUS Lane: θ–γ phase-field bulk imprint + resonance recall."""
        cls._enable_chorus_mode()
        native = _require_native()
        if path:
            brain = native.Brain.open_readonly(path) if readonly else native.Brain.open(path)
            obj = cls(brain, readonly=readonly, mode=cls.MODE_CHORUS)
            obj.brain_path = path
            return obj
        return cls(native.Brain.new(), readonly=False, mode=cls.MODE_CHORUS)

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
    def connect_agent_fast(cls, path: str, *, readonly: bool = False) -> "FluctlightBrain":
        """Agent memory with research-backed fast recall (hybrid index + shallow spread).

        Keeps full episodic write path (dentate, provenance, graph wiring). For recall,
        pre-filters via FTS5+HNSW sidecar (``FLUCTLIGHT_CANDIDATE_CAP``) and limits graph
        spread to 1 hop. Rebuild sidecar after bulk ingest: ``fluctlight index rebuild``.
        """
        cls._enable_agent_fast_mode()
        native = _require_native()
        brain = native.Brain.open_readonly(path) if readonly else native.Brain.open(path)
        obj = cls(brain, readonly=readonly, mode=cls.MODE_AGENT_FAST)
        obj.brain_path = path
        return obj

    @classmethod
    def connect_brain(cls, path: Optional[str] = None, *, readonly: bool = False) -> "FluctlightBrain":
        """Brain-native agent memory: dentate separation, graph spread, CLS sleep, cortex boost.

        Use for LongMemEval E2E and production agent recall — not the stripped ``connect_index()``
        IR-only path (which disables graph spread for Chroma-class latency).
        """
        cls._enable_brain_mode()
        native = _require_native()
        if path:
            brain = native.Brain.open_readonly(path) if readonly else native.Brain.open(path)
            obj = cls(brain, readonly=readonly, mode=cls.MODE_BRAIN)
            obj.brain_path = path
            return obj
        return cls(native.Brain.new(), readonly=False, mode=cls.MODE_BRAIN)

    @classmethod
    def connect_muon(cls, path: Optional[str] = None, *, readonly: bool = False) -> "FluctlightBrain":
        """Muon Lane: penetrative bulk session imprint — haystack replacement (0 embed HTTP)."""
        cls._enable_muon_mode()
        native = _require_native()
        if path:
            brain = native.Brain.open_readonly(path) if readonly else native.Brain.open(path)
            obj = cls(brain, readonly=readonly, mode=cls.MODE_BRAIN)
            obj.brain_path = path
            return obj
        return cls(native.Brain.new(), readonly=False, mode=cls.MODE_BRAIN)

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

    def sleep_cycles(self, n: int = 2) -> list[dict[str, Any]]:
        reports: list[dict[str, Any]] = []
        for _ in range(max(0, n)):
            reports.append(self.sleep())
        return reports

    def complete(self, cue: str, *, limit: Optional[int] = None) -> Optional[dict[str, Any]]:
        """CA3 pattern completion — best engram for partial cue."""
        fn = getattr(self._brain, "complete", None)
        if fn is None:
            return None
        return fn(cue, limit)

    def cortex_facts(self, cue: str, *, limit: int = 24) -> list[Any]:
        fn = getattr(self._brain, "cortex_facts", None)
        if fn is None:
            return []
        return fn(cue, limit)

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

    def muon_imprint(
        self,
        session_id: str,
        body: str,
        *,
        date: str = "",
        user_keys: str = "",
    ) -> None:
        self._brain.muon_imprint(session_id, date, body, user_keys)

    def muon_imprint_batch(self, sessions: list[dict[str, str]]) -> int:
        return int(self._brain.muon_imprint_batch_json(json.dumps(sessions)))

    def muon_recall(self, cue: str, *, limit: int = 8) -> list[dict[str, Any]]:
        raw = self._brain.muon_recall(cue, limit)
        if isinstance(raw, list):
            return raw
        return []

    def muon_len(self) -> int:
        return int(self._brain.muon_len())

    def tau_recall(
        self, cue: str, *, limit: int = 8, question_type: str | None = None
    ) -> list[dict[str, Any]]:
        raw = self._brain.tau_recall(cue, limit, question_type or "")
        if isinstance(raw, list):
            return raw
        return []

    def tau_recall_rrf(
        self,
        cues: list[str],
        *,
        limit: int = 8,
        question_type: str | None = None,
    ) -> list[dict[str, Any]]:
        raw = self._brain.tau_recall_rrf(cues, limit, question_type or "")
        if isinstance(raw, list):
            return raw
        return []

    def tau_shard_len(self) -> int:
        return int(self._brain.tau_shard_len())

    def tau_crystallize_shard(self, shard_id: str) -> str:
        return str(self._brain.tau_crystallize_shard(shard_id))

    def chorus_imprint_batch(self, batch: list[dict[str, Any]]) -> int:
        return int(self._brain.chorus_imprint_batch_json(json.dumps(batch)))

    def chorus_recall(
        self,
        cue: str,
        *,
        limit: int = 8,
        semantic_vector: Optional[list[float]] = None,
        tag: bool = False,
        fast: Optional[bool] = None,
    ) -> list[Any]:
        raw = self._brain.chorus_recall(cue, limit, semantic_vector, fast, tag)
        if isinstance(raw, list):
            return raw
        return []

    def chorus_recall_batch(
        self,
        cues: list[str],
        embeddings: list[list[float]],
        *,
        limit: int = 8,
        fast: Optional[bool] = None,
    ) -> list[list[Any]]:
        if not cues:
            return []
        dim = len(embeddings[0]) if embeddings else 0
        try:
            import numpy as np

            arr = np.ascontiguousarray(embeddings, dtype=np.float32)
            flat = arr.ravel().tolist()
            raw = self._brain.chorus_recall_batch_flat(cues, flat, dim, limit, fast)
        except ImportError:
            flat: list[float] = [x for row in embeddings for x in row]
            raw = self._brain.chorus_recall_batch_flat(cues, flat, dim, limit, fast)
        return raw if isinstance(raw, list) else []

    # --- Late interaction: token-population MaxSim ⊕ BM25 (RRF) ---

    @staticmethod
    def _flatten_tokens(
        token_batches: list[list[list[float]]],
    ) -> tuple[list[float], list[int], int]:
        """Flatten a list of per-item token matrices into (flat, counts, dim)."""
        counts = [len(toks) for toks in token_batches]
        dim = 0
        for toks in token_batches:
            if toks:
                dim = len(toks[0])
                break
        flat: list[float] = []
        for toks in token_batches:
            for row in toks:
                flat.extend(row)
        return flat, counts, dim

    def chorus_imprint_maxsim(self, items: list[dict[str, Any]]) -> int:
        """Imprint traces with per-token vectors for MaxSim late interaction.

        Each item: {memory_id, content, token_vectors: list[list[float]],
        context?: str, salience?: float}. The pooled photon vector is derived
        from the tokens natively.
        """
        if not items:
            return 0
        memory_ids = [str(it["memory_id"]) for it in items]
        contents = [str(it.get("content", "")) for it in items]
        contexts = [str(it.get("context", it["memory_id"])) for it in items]
        token_batches = [list(it.get("token_vectors") or []) for it in items]
        salience = float(items[0].get("salience", 0.62))
        flat, counts, dim = self._flatten_tokens(token_batches)
        return int(
            self._brain.chorus_imprint_maxsim_batch(
                memory_ids, contents, contexts, flat, counts, dim, salience
            )
        )

    def chorus_recall_maxsim(
        self,
        cues: list[str],
        query_token_vectors: list[list[list[float]]],
        *,
        limit: int = 150,
        w_bm: float = 0.7,
    ) -> list[list[Any]]:
        """Batch MaxSim⊕BM25 recall. `cues[i]` drives BM25;
        `query_token_vectors[i]` is that query's per-token vector matrix.
        Returns per-query lists of (memory_id, score)."""
        if not cues:
            return []
        flat, counts, dim = self._flatten_tokens(query_token_vectors)
        raw = self._brain.chorus_recall_maxsim_batch(cues, flat, counts, dim, limit, w_bm)
        return raw if isinstance(raw, list) else []

    def chorus_sleep(self) -> dict[str, Any]:
        raw = self._brain.chorus_sleep()
        return raw if isinstance(raw, dict) else {}

    def chorus_tick(self) -> int:
        return int(self._brain.chorus_tick())

    def chorus_len(self) -> int:
        return int(self._brain.chorus_len())

  # --- Agent ergonomics (WM-Ring, unified recall, tool observe, retention) ---

    def turn_begin(self) -> None:
        """Start agent turn — WM-Ring tracks this conversation slice."""
        self._brain.turn_begin()

    def turn_end(self, *, flush: bool = True) -> dict[str, Any]:
        """End turn; optionally flush working memory to hippocampus."""
        raw = self._brain.turn_end(flush)
        return raw if isinstance(raw, dict) else {}

    def wm_push(
        self,
        content: str,
        *,
        context: str = "turn",
        salience: float = 0.6,
        semantic_vector: Optional[list[float]] = None,
    ) -> None:
        self._brain.wm_push(content, context, salience, semantic_vector)

    def wm_len(self) -> int:
        return int(self._brain.wm_len())

    def observe_tool(
        self,
        tool_name: str,
        result: str,
        *,
        uri: Optional[str] = None,
        context: Optional[str] = None,
        salience: float = 0.72,
        semantic_vector: Optional[list[float]] = None,
        to_working_memory: bool = False,
    ) -> dict[str, Any]:
        """Ingest MCP/tool output with ToolGrounded provenance."""
        payload = {
            "tool_name": tool_name,
            "result": result,
            "uri": uri,
            "context": context,
            "salience": salience,
            "semantic_vector": semantic_vector,
            "to_working_memory": to_working_memory,
        }
        raw = self._brain.observe_tool_json(json.dumps(payload))
        return raw if isinstance(raw, dict) else {}

    def recall(
        self,
        cue: str,
        *,
        mode: str = "auto",
        limit: int = 8,
        semantic_vector: Optional[list[float]] = None,
        tick_from: Optional[int] = None,
        tick_to: Optional[int] = None,
    ) -> dict[str, Any]:
        """Unified recall — auto-routes episodic / corpus / session lanes.

        Pass ``tick_from`` / ``tick_to`` for Chronos temporal gate (explicit window).
        Natural cues like "last week" / "yesterday" also trigger automatic temporal filter.
        """
        fn = getattr(self._brain, "recall_unified", None)
        if fn is None:
            return {"hits": [], "mode": mode, "lanes_used": []}
        raw = fn(cue, mode, limit, semantic_vector, tick_from, tick_to)
        return raw if isinstance(raw, dict) else {"hits": [], "mode": mode, "lanes_used": []}

    def query(self, op: dict[str, Any]) -> dict[str, Any]:
        """Brain-native query layer (list_engrams, forget, stats, …)."""
        qfn = getattr(self._brain, "query_json", None)
        if qfn is None:
            qmut = getattr(self._brain, "query_mut_json", None)
            if qmut and op.get("op") in ("forget", "forget_before"):
                raw = qmut(json.dumps(op))
            else:
                raise RuntimeError("query requires fluctlightdb-native with query_json")
            return raw if isinstance(raw, dict) else {}
        if op.get("op") in ("forget", "forget_before"):
            qmut = getattr(self._brain, "query_mut_json", None)
            if qmut:
                raw = qmut(json.dumps(op))
                return raw if isinstance(raw, dict) else {}
        raw = qfn(json.dumps(op))
        return raw if isinstance(raw, dict) else {}

    def export_snapshot(self) -> str:
        fn = getattr(self._brain, "export_snapshot_json", None)
        if fn is None:
            raise RuntimeError("export_snapshot requires fluctlightdb-native")
        return str(fn())

    def import_snapshot(self, json_blob: str) -> dict[str, Any]:
        fn = getattr(self._brain, "import_snapshot_json", None)
        if fn is None:
            raise RuntimeError("import_snapshot requires fluctlightdb-native")
        raw = fn(json_blob)
        return raw if isinstance(raw, dict) else {}

    def scrub_pii(self) -> dict[str, Any]:
        fn = getattr(self._brain, "scrub_pii", None)
        if fn is None:
            raise RuntimeError("scrub_pii requires fluctlightdb-native")
        raw = fn()
        return raw if isinstance(raw, dict) else {}

    def delete_by_subject(self, subject: str) -> dict[str, Any]:
        fn = getattr(self._brain, "delete_by_subject", None)
        if fn is None:
            raise RuntimeError("delete_by_subject requires fluctlightdb-native")
        raw = fn(subject)
        return raw if isinstance(raw, dict) else {}

    def delete_by_agent_id(self, agent_id: str) -> int:
        fn = getattr(self._brain, "delete_by_agent_id", None)
        if fn is None:
            raise RuntimeError("delete_by_agent_id requires fluctlightdb-native")
        return int(fn(agent_id))

    def audit_log(self, limit: int = 50) -> list[dict[str, Any]]:
        fn = getattr(self._brain, "audit_log_json", None)
        if fn is None:
            return []
        raw = fn(limit)
        return raw if isinstance(raw, list) else []

    @staticmethod
    def replicate_sync(primary: str, replica: str) -> dict[str, Any]:
        """Incremental brain replica sync (VPS hub / laptop spoke)."""
        native = _require_native()
        raw = native.Brain.replicate_sync(primary, replica)
        return raw if isinstance(raw, dict) else {}

    def resolve(
        self,
        cue: str,
        *,
        semantic_vector: Optional[list[float]] = None,
    ) -> dict[str, Any]:
        """Conflict lattice — pick the trusted fact when memories disagree."""
        raw = self._brain.resolve(cue, semantic_vector)
        return raw if isinstance(raw, dict) else {}

    def retain_for(
        self,
        *,
        days: Optional[int] = 30,
        unless_verified: bool = True,
        min_salience: Optional[float] = None,
    ) -> None:
        """Retention DSL — e.g. retain_for(days=30, unless_verified=True)."""
        self._brain.retain_for(days, unless_verified, min_salience)

    def consolidate(self) -> dict[str, Any]:
        """Manual sleep: flush WM + CHORUS collapse + hippocampal sleep + retention."""
        raw = self._brain.consolidate()
        return raw if isinstance(raw, dict) else {}

    def set_auto_consolidate(self, enabled: bool = True) -> None:
        """Enable idle auto-consolidation on tick() (default on for connect_agent)."""
        self._brain.set_auto_consolidate(enabled)


def connect(path: str, *, readonly: bool = False) -> FluctlightBrain:
    """Open an agent brain directory (full episodic memory path)."""
    return FluctlightBrain.connect(path, readonly=readonly)


def connect_brain(path: Optional[str] = None, *, readonly: bool = False) -> FluctlightBrain:
    """Brain-native memory (full episodic path + graph + cortex). Prefer over ``connect_index()`` for agents."""
    return FluctlightBrain.connect_brain(path, readonly=readonly)


def connect_index(path: Optional[str] = None, *, readonly: bool = False) -> FluctlightBrain:
    """Open a bulk semantic index (fast ingest + vector recall)."""
    return FluctlightBrain.connect_index(path, readonly=readonly)


def connect_conv(path: Optional[str] = None, *, readonly: bool = False) -> FluctlightBrain:
    """Conversational RAG mode: fast ingest + hybrid recall."""
    return FluctlightBrain.connect_conv(path, readonly=readonly)


def connect_agent_fast(path: str, *, readonly: bool = False) -> FluctlightBrain:
    """Agent memory with fast hybrid recall (see :meth:`FluctlightBrain.connect_agent_fast`)."""
    return FluctlightBrain.connect_agent_fast(path, readonly=readonly)


def connect_muon(path: Optional[str] = None, *, readonly: bool = False) -> FluctlightBrain:
    """Muon Lane bulk imprint — session-level haystack ingest without per-turn encode."""
    return FluctlightBrain.connect_muon(path, readonly=readonly)


def connect_agent(path: Optional[str] = None, *, readonly: bool = False, retain_days: Optional[int] = 30) -> FluctlightBrain:
    """Recommended unified agent brain — WM-Ring, auto recall routing, auto-consolidate."""
    return FluctlightBrain.connect_agent(path, readonly=readonly, retain_days=retain_days)


def connect_embedded(
    path: str,
    *,
    readonly: bool = False,
    retain_days: Optional[int] = 30,
    secure_dir: bool = True,
) -> FluctlightBrain:
    """Production embedded brain — prefer over ``connect_agent()`` for shipped single-process agents."""
    return FluctlightBrain.connect_embedded(
        path,
        readonly=readonly,
        retain_days=retain_days,
        secure_dir=secure_dir,
    )


def connect_chorus(path: Optional[str] = None, *, readonly: bool = False) -> FluctlightBrain:
    """CHORUS phase-field memory — light-speed binary imprint + resonance recall."""
    return FluctlightBrain.connect_chorus(path, readonly=readonly)
