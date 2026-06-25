"""HTTP hub client for VPS-hosted project brains (desktop + Cursor CLI)."""

from __future__ import annotations

import json
import os
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Optional

from .handoff import Handoff
from .handoff_index import append_handoff, list_handoffs as index_list_handoffs
from .project import ProjectConfig, find_project_root


@dataclass
class RemoteProjectBrains:
    """Project brain via fluctlight-serve (shared VPS hub)."""

    config: ProjectConfig
    agent: str
    subdir: str
    session_id: str
    client: Any
    local_index: bool = True

    @classmethod
    def connect(
        cls,
        *,
        start: Optional[os.PathLike[str] | str] = None,
        agent: Optional[str] = None,
        session_id: Optional[str] = None,
        hub_url: Optional[str] = None,
        api_key: Optional[str] = None,
    ) -> "RemoteProjectBrains":
        from .handoff import detect_agent
        from . import FluctlightClient

        root = find_project_root(start)
        cfg = ProjectConfig.load(root)
        agent_name = detect_agent(agent or str(cfg.defaults.get("agent", "auto")))
        if agent_name == "unknown":
            agent_name = "cursor"
        rel_sub = ""
        try:
            rel_sub = str(Path(start or os.getcwd()).resolve().relative_to(root))  # type: ignore[name-defined]
        except ValueError:
            rel_sub = ""
        if rel_sub == ".":
            rel_sub = ""
        sid = session_id or os.environ.get("FLUCTLIGHT_SESSION_ID", "")[:12]

        serve = cfg.serve or {}
        url = (
            hub_url
            or os.environ.get("FLUCTLIGHT_HUB_URL", "")
            or os.environ.get("FLUCTLIGHT_SERVE_URL", "")
            or str(serve.get("url", ""))
        ).rstrip("/")
        if not url:
            raise ValueError(
                "Remote hub URL required — set FLUCTLIGHT_HUB_URL or serve.url in config.yaml"
            )
        key = api_key or os.environ.get("FLUCTLIGHT_API_KEY", "") or str(serve.get("api_key", ""))
        client = FluctlightClient(base_url=url, api_key=key)
        if not client.health():
            raise ConnectionError(f"Fluctlight hub unreachable at {url}")

        return cls(
            config=cfg,
            agent=agent_name,
            subdir=rel_sub.replace("\\", "/"),
            session_id=sid,
            client=client,
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
        **extra: Any,
    ) -> dict[str, Any]:
        from .validation import validate_content

        validate_content(content)
        ctx = context
        if self.subdir and context == "session":
            ctx = f"session:{self.agent}:{self.subdir}"
        return self.client.experience(
            content,
            context=ctx,
            salience=salience,
            agent_id=self.agent if scope == "agent" else None,
            tenant_id=self._tenant_id(),
            **extra,
        )

    def recall(
        self,
        cue: str,
        *,
        scope: str = "all",
        limit: Optional[int] = 12,
    ) -> dict[str, Any]:
        rep = self.client.activate(cue, agent_id=self.agent if scope != "project" else None)
        recalls = rep.get("recalls") or []
        if limit:
            recalls = recalls[:limit]
        return {"agent": self.agent, "subdir": self.subdir, "recalls": recalls, "count": len(recalls)}

    def handoff(
        self,
        summary: str,
        *,
        next_steps: Optional[list[str]] = None,
        status: str = "paused",
        files: Optional[list[str]] = None,
        to_project: bool = True,
    ) -> Handoff:
        from .validation import validate_content

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
        self.client.experience(
            h.to_content(),
            context=h.context_key(),
            salience=0.92,
            agent_id=self.agent,
            tenant_id=self._tenant_id(),
        )
        if self.local_index:
            append_handoff(self.config.fluctlight_root, h)
        return h

    def list_handoffs(self, **kwargs: Any) -> list[Handoff]:
        local = index_list_handoffs(self.config.fluctlight_root, **kwargs)
        if local:
            return local
        raw = self.recall("handoff", scope="project", limit=kwargs.get("limit", 20) * 3)
        out: list[Handoff] = []
        for item in raw.get("recalls") or []:
            ep = item.get("episode") or {}
            content = ep.get("content") or ""
            ctx = str(ep.get("context") or "")
            if not ctx.startswith("handoff:"):
                continue
            try:
                out.append(Handoff.from_content(content))
            except (json.JSONDecodeError, TypeError):
                continue
        return out[: kwargs.get("limit", 20)]

    def recall_handoffs(self, *, subdir: Optional[str] = None, limit: int = 8) -> list[Handoff]:
        return self.list_handoffs(
            subdir=subdir if subdir is not None else self.subdir or None,
            limit=limit,
        )

    def session_context(self, *, limit: int = 10) -> str:
        lines = [
            f"# FluctlightDB project: {self.config.project_id} (remote hub)",
            f"Agent: {self.agent} | subdir: {self.subdir or '/'} | hub: {self.client.base_url}",
            "",
        ]
        handoffs = self.recall_handoffs(limit=5)
        if handoffs:
            lines.append("## Recent handoffs")
            for h in handoffs:
                lines.append(f"- [{h.handoff_id}] {h.format_brief()}")
            lines.append("")
        lines.append("## Recalled memories")
        for cue in (f"project conventions {self.config.project_id}", "architecture decisions"):
            for item in self.recall(cue, limit=limit).get("recalls") or []:
                ep = item.get("episode") or {}
                content = (ep.get("content") or "").strip()
                if content and not content.startswith("{"):
                    lines.append(f"- {content[:400]}")
                    break
        return "\n".join(lines).strip()

    def status(self) -> dict[str, Any]:
        st = self.client.status()
        return {
            "project_id": self.config.project_id,
            "root": str(self.config.root),
            "agent": self.agent,
            "subdir": self.subdir,
            "hub_url": self.client.base_url,
            "remote": True,
            "serve_status": st,
            "handoffs": [{"id": h.handoff_id, "brief": h.format_brief()} for h in self.recall_handoffs(limit=6)],
        }

    def checkpoint(self) -> None:
        pass  # server persists
