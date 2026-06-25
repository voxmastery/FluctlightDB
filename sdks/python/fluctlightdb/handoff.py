"""Structured handoffs between agents (Cursor, Claude Code, Codex, …)."""

from __future__ import annotations

import json
import os
import uuid
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
from typing import Any, Optional


HANDOFF_PREFIX = "handoff:"


@dataclass
class Handoff:
    agent: str
    subdir: str = ""
    session_id: str = ""
    status: str = "paused"  # paused | done | blocked
    summary: str = ""
    next_steps: list[str] = field(default_factory=list)
    files: list[str] = field(default_factory=list)
    handoff_id: str = field(default_factory=lambda: str(uuid.uuid4())[:8])
    created_at: str = field(
        default_factory=lambda: datetime.now(timezone.utc).isoformat(timespec="seconds")
    )

    def context_key(self) -> str:
        sub = self.subdir.strip("/").replace("/", ":") or "root"
        return f"{HANDOFF_PREFIX}{self.agent}:{sub}"

    def to_content(self) -> str:
        payload = asdict(self)
        return json.dumps(payload, ensure_ascii=False, separators=(",", ":"))

    @classmethod
    def from_content(cls, content: str) -> "Handoff":
        data = json.loads(content)
        return cls(
            agent=str(data.get("agent", "unknown")),
            subdir=str(data.get("subdir", "")),
            session_id=str(data.get("session_id", "")),
            status=str(data.get("status", "paused")),
            summary=str(data.get("summary", "")),
            next_steps=list(data.get("next_steps") or []),
            files=list(data.get("files") or []),
            handoff_id=str(data.get("handoff_id", "")),
            created_at=str(data.get("created_at", "")),
        )

    def format_brief(self) -> str:
        lines = [
            f"[{self.agent} @ {self.subdir or '/'}] {self.status}: {self.summary}",
        ]
        if self.next_steps:
            lines.append("Next: " + "; ".join(self.next_steps[:5]))
        if self.files:
            lines.append("Files: " + ", ".join(self.files[:8]))
        return "\n".join(lines)


def detect_agent(explicit: Optional[str] = None) -> str:
    if explicit and explicit != "auto":
        return explicit.lower()
    for key in ("FLUCTLIGHT_AGENT", "CURSOR_AGENT", "CLAUDE_CODE_AGENT"):
        val = os.environ.get(key, "").strip().lower()
        if val:
            return val
    if os.environ.get("CURSOR_SESSION_ID") or os.environ.get("CURSOR_TRACE_ID"):
        return "cursor"
    if os.environ.get("CLAUDE_CODE") or (
        os.environ.get("ANTHROPIC_API_KEY") and os.environ.get("CLAUDE_CODE_SSE_PORT")
    ):
        return "claude"
    if os.environ.get("CODEX_HOME") or os.environ.get("OPENAI_CODEX"):
        return "codex"
    return "unknown"
