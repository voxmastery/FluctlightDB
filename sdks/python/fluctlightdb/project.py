"""Project hub + agent spoke brains for multi-tool monorepos."""

from __future__ import annotations

import json
import logging
import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Optional

import yaml

from .handoff import HANDOFF_PREFIX, Handoff, detect_agent
from .handoff_index import append_handoff, get_handoff, list_handoffs as index_list_handoffs
from .lock import brain_write_lock
from .platform import normalize_subdir
from .validation import validate_content, warn_serve_embedded_conflict

logger = logging.getLogger(__name__)

CONFIG_FILE = "config.yaml"
FLUCTLIGHT_DIR = ".fluctlight"


@dataclass
class ProjectConfig:
    version: int = 1
    project_id: str = "project"
    brains: dict[str, Any] = field(default_factory=dict)
    serve: dict[str, Any] = field(default_factory=dict)
    defaults: dict[str, Any] = field(default_factory=dict)
    git: dict[str, Any] = field(default_factory=dict)
    root: Path = field(default_factory=Path.cwd)

    @property
    def fluctlight_root(self) -> Path:
        return self.root / FLUCTLIGHT_DIR

    def brain_path(self, key: str) -> Path:
        rel = self.brains.get(key)
        if rel is None and key == "project":
            rel = "project"
        if rel is None:
            agents = self.brains.get("agents") or {}
            rel = agents.get(key)
        if rel is None:
            raise KeyError(f"unknown brain key: {key}")
        p = Path(rel)
        if not p.is_absolute():
            p = self.fluctlight_root / p
        return p

    def agent_names(self) -> list[str]:
        agents = self.brains.get("agents") or {}
        return list(agents.keys())

    @classmethod
    def load(cls, root: Path) -> "ProjectConfig":
        cfg_path = root / FLUCTLIGHT_DIR / CONFIG_FILE
        if not cfg_path.is_file():
            raise FileNotFoundError(f"no {FLUCTLIGHT_DIR}/{CONFIG_FILE} under {root}")
        data = yaml.safe_load(cfg_path.read_text(encoding="utf-8")) or {}
        return cls(
            version=int(data.get("version", 1)),
            project_id=str(data.get("project_id", root.name)),
            brains=dict(data.get("brains") or {}),
            serve=dict(data.get("serve") or {}),
            defaults=dict(data.get("defaults") or {}),
            git=dict(data.get("git") or {}),
            root=root.resolve(),
        )

    def to_dict(self) -> dict[str, Any]:
        return {
            "version": self.version,
            "project_id": self.project_id,
            "brains": self.brains,
            "serve": self.serve,
            "defaults": self.defaults,
            "git": self.git,
        }


def find_project_root(start: Optional[os.PathLike[str] | str] = None) -> Path:
    """Walk up from *start* (or cwd) until `.fluctlight/config.yaml` exists."""
    cur = Path(start or os.getcwd()).resolve()
    for directory in (cur, *cur.parents):
        if (directory / FLUCTLIGHT_DIR / CONFIG_FILE).is_file():
            return directory
    raise FileNotFoundError(
        f"no {FLUCTLIGHT_DIR}/{CONFIG_FILE} found from {cur} — run: fluctlight-project init"
    )


def _open_brain(path: Path, *, readonly: bool = False):
    from .brain import connect

    path.mkdir(parents=True, exist_ok=True)
    return connect(str(path), readonly=readonly)


@dataclass
class ProjectBrains:
    """Hub (project) + spoke (agent) brains for one working session."""

    config: ProjectConfig
    agent: str
    subdir: str
    session_id: str
    project_brain: Any
    agent_brain: Any

    @classmethod
    def connect(
        cls,
        *,
        start: Optional[os.PathLike[str] | str] = None,
        agent: Optional[str] = None,
        session_id: Optional[str] = None,
    ) -> "ProjectBrains":
        root = find_project_root(start)
        cfg = ProjectConfig.load(root)
        agent_name = detect_agent(agent or str(cfg.defaults.get("agent", "auto")))
        if agent_name == "unknown":
            agent_name = "cursor"
        rel_sub = "."
        try:
            rel_sub = str(Path(start or os.getcwd()).resolve().relative_to(root))
        except ValueError:
            rel_sub = "."
        if rel_sub == ".":
            rel_sub = ""
        sid = session_id or os.environ.get("FLUCTLIGHT_SESSION_ID", "") or os.environ.get(
            "CURSOR_SESSION_ID", ""
        )[:12]
        warn = warn_serve_embedded_conflict()
        if warn:
            logger.warning(warn)
        project_path = cfg.brain_path("project")
        if agent_name in cfg.agent_names():
            agent_path = cfg.brain_path(agent_name)
        else:
            agents_dir = cfg.fluctlight_root / "agents" / agent_name
            agents_dir.mkdir(parents=True, exist_ok=True)
            agent_path = agents_dir
        return cls(
            config=cfg,
            agent=agent_name,
            subdir=rel_sub.replace("\\", "/"),
            session_id=sid,
            project_brain=_open_brain(project_path),
            agent_brain=_open_brain(agent_path),
        )

    def _tenant_id(self) -> str:
        return self.config.project_id

    def remember(
        self,
        content: str,
        *,
        scope: str = "agent",
        context: str = "session",
        salience: float = 0.55,
        verified: Optional[bool] = None,
        provenance_kind: Optional[str] = None,
        source_uri: Optional[str] = None,
        **extra: Any,
    ) -> dict[str, Any]:
        validate_content(content)
        ctx = context
        if self.subdir and context == "session":
            ctx = f"session:{self.agent}:{self.subdir}"
        target = self.agent_brain if scope == "agent" else self.project_brain
        path = getattr(target, "brain_path", None) or ""
        if not path:
            result = target.experience(
                content,
                context=ctx,
                salience=salience,
                agent_id=self.agent,
                tenant_id=self._tenant_id(),
                verified=verified,
                provenance_kind=provenance_kind,
                source_uri=source_uri,
                **extra,
            )
            return result if isinstance(result, dict) else {"ok": True}
        with brain_write_lock(path):
            result = target.experience(
                content,
                context=ctx,
                salience=salience,
                agent_id=self.agent,
                tenant_id=self._tenant_id(),
                verified=verified,
                provenance_kind=provenance_kind,
                source_uri=source_uri,
                **extra,
            )
            return result if isinstance(result, dict) else {"ok": True}

    def recall(
        self,
        cue: str,
        *,
        scope: str = "all",
        limit: Optional[int] = 12,
    ) -> dict[str, Any]:
        merged: list[dict[str, Any]] = []
        meta: dict[str, Any] = {"agent": self.agent, "subdir": self.subdir}
        if scope in ("all", "project"):
            pr = self.project_brain.activate(cue, limit=limit)
            merged.extend(self._extract_recalls(pr))
        if scope in ("all", "agent"):
            ar = self.agent_brain.activate(cue, agent_id=self.agent, limit=limit)
            merged.extend(self._extract_recalls(ar))
        merged.sort(key=lambda r: float(r.get("activation", 0) or 0), reverse=True)
        if limit:
            merged = merged[:limit]
        meta["recalls"] = merged
        meta["count"] = len(merged)
        return meta

    @staticmethod
    def _extract_recalls(payload: Any) -> list[dict[str, Any]]:
        if isinstance(payload, dict):
            return list(payload.get("recalls") or [])
        if isinstance(payload, str):
            try:
                data = json.loads(payload)
                return list(data.get("recalls") or [])
            except json.JSONDecodeError:
                return []
        return []

    def handoff(
        self,
        summary: str,
        *,
        next_steps: Optional[list[str]] = None,
        status: str = "paused",
        files: Optional[list[str]] = None,
        to_project: bool = True,
    ) -> Handoff:
        validate_content(summary, field="summary")
        h = Handoff(
            agent=self.agent,
            subdir=self.subdir,
            session_id=self.session_id,
            status=status,
            summary=summary,
            next_steps=list(next_steps or []),
            files=list(files or []),
        )
        self.remember(
            h.to_content(),
            scope="project" if to_project else "agent",
            context=h.context_key(),
            salience=0.92,
        )
        append_handoff(self.config.fluctlight_root, h)
        return h

    def list_handoffs(
        self,
        *,
        agent: Optional[str] = None,
        subdir: Optional[str] = None,
        status: Optional[str] = None,
        since: Optional[str] = None,
        limit: int = 20,
    ) -> list[Handoff]:
        return index_list_handoffs(
            self.config.fluctlight_root,
            agent=agent,
            subdir=subdir,
            status=status,
            since=since,
            limit=limit,
        )

    def get_handoff(self, handoff_id: str) -> Optional[Handoff]:
        return get_handoff(self.config.fluctlight_root, handoff_id)

    def recall_handoffs(self, *, subdir: Optional[str] = None, limit: int = 8) -> list[Handoff]:
        indexed = self.list_handoffs(
            subdir=subdir if subdir is not None else self.subdir or None,
            limit=limit,
        )
        if indexed:
            return indexed
        return self._recall_handoffs_legacy(subdir=subdir, limit=limit)

    def _recall_handoffs_legacy(self, *, subdir: Optional[str] = None, limit: int = 8) -> list[Handoff]:
        cue = "handoff"
        if subdir is not None:
            cue = f"handoff {subdir.strip('/')}"
        elif self.subdir:
            cue = f"handoff {self.subdir.strip('/')}"
        raw = self.recall(cue, scope="project", limit=limit * 3)
        out: list[Handoff] = []
        for item in raw.get("recalls") or []:
            ep = item.get("episode") or {}
            content = ep.get("content") or ""
            ctx = str(ep.get("context") or "")
            if not ctx.startswith(HANDOFF_PREFIX):
                continue
            try:
                out.append(Handoff.from_content(content))
            except (json.JSONDecodeError, TypeError):
                continue
        return out[:limit]

    def session_context(self, *, limit: int = 10) -> str:
        """Compact block for hooks / MCP / system prompt injection."""
        lines = [
            f"# FluctlightDB project: {self.config.project_id}",
            f"Agent: {self.agent} | subdir: {self.subdir or '/'} | session: {self.session_id or '—'}",
            "",
        ]
        handoffs = self.recall_handoffs(limit=5)
        if handoffs:
            lines.append("## Recent handoffs (other agents)")
            for h in handoffs:
                lines.append(f"- [{h.handoff_id}] {h.format_brief()}")
            lines.append("")
        cues = [
            f"project conventions {self.config.project_id}",
            f"decisions {self.subdir}" if self.subdir else "architecture decisions",
        ]
        seen = set()
        lines.append("## Recalled memories")
        for cue in cues:
            for item in self.recall(cue, scope="all", limit=limit).get("recalls") or []:
                ep = item.get("episode") or {}
                content = (ep.get("content") or "").strip()
                if not content or content in seen:
                    continue
                if content.startswith("{"):
                    continue
                seen.add(content)
                lines.append(f"- {content[:400]}")
                if len(seen) >= limit:
                    break
            if len(seen) >= limit:
                break
        return "\n".join(lines).strip()

    def status(self) -> dict[str, Any]:
        return {
            "project_id": self.config.project_id,
            "root": str(self.config.root),
            "agent": self.agent,
            "subdir": self.subdir,
            "session_id": self.session_id,
            "agents": self.config.agent_names(),
            "handoffs": [
                {"id": h.handoff_id, "brief": h.format_brief()} for h in self.recall_handoffs(limit=6)
            ],
        }

    def checkpoint(self) -> None:
        for brain in (self.project_brain, self.agent_brain):
            path = getattr(brain, "brain_path", None) or ""
            try:
                if not path:
                    brain.checkpoint()
                    continue
                with brain_write_lock(path):
                    brain.checkpoint()
            except RuntimeError as exc:
                if "lock busy" not in str(exc).lower():
                    raise


def connect_project(
    *,
    start: Optional[os.PathLike[str] | str] = None,
    agent: Optional[str] = None,
    session_id: Optional[str] = None,
) -> ProjectBrains:
    """Connect hub + spoke brains for the repo containing *start* (or cwd)."""
    return ProjectBrains.connect(start=start, agent=agent, session_id=session_id)
