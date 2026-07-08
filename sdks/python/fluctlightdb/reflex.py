"""Reflex auto-ingest — post-turn memory encoding without explicit agent calls."""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Any, Optional


_SKIP_PATTERNS = (
    re.compile(r"^(ok|thanks|thank you|sure|yes|no|done|got it)[\s!.]*$", re.I),
    re.compile(r"^```", re.MULTILINE),
)


def _worth_remembering(text: str, *, min_chars: int = 24) -> bool:
    stripped = text.strip()
    if len(stripped) < min_chars:
        return False
    for pat in _SKIP_PATTERNS:
        if pat.search(stripped):
            return False
    return True


@dataclass
class ToolTurnResult:
    name: str
    result: str
    uri: Optional[str] = None


@dataclass
class ReflexConfig:
    """Tune automatic post-turn ingest."""

    min_user_chars: int = 12
    min_assistant_chars: int = 24
    tool_salience: float = 0.74
    user_salience: float = 0.58
    assistant_salience: float = 0.52
    flush: bool = True
    extract_bullets: bool = True


@dataclass
class ReflexReport:
    wm_pushed: int = 0
    tools_observed: int = 0
    flushed: dict[str, Any] = field(default_factory=dict)


def reflex_ingest_turn(
    brain: Any,
    *,
    user_text: Optional[str] = None,
    assistant_text: Optional[str] = None,
    tools: Optional[list[ToolTurnResult | dict[str, Any]]] = None,
    config: Optional[ReflexConfig] = None,
) -> ReflexReport:
    """Encode a completed agent turn into WM-Ring / hippocampus automatically."""
    cfg = config or ReflexConfig()
    report = ReflexReport()
    brain.turn_begin()

    if user_text and len(user_text.strip()) >= cfg.min_user_chars:
        brain.wm_push(user_text.strip(), context="user", salience=cfg.user_salience)
        report.wm_pushed += 1

    for raw in tools or []:
        if isinstance(raw, ToolTurnResult):
            tname, tres, uri = raw.name, raw.result, raw.uri
        else:
            tname = str(raw.get("name") or raw.get("tool_name") or "tool")
            tres = str(raw.get("result") or raw.get("output") or "")
            uri = raw.get("uri")
        if not tres.strip():
            continue
        brain.observe_tool(tname, tres[:8000], uri=uri, salience=cfg.tool_salience)
        report.tools_observed += 1

    if assistant_text and _worth_remembering(assistant_text, min_chars=cfg.min_assistant_chars):
        if cfg.extract_bullets:
            for line in assistant_text.splitlines():
                line = line.strip(" •-\t")
                if line.startswith(("-", "*", "•")) or _worth_remembering(line, min_chars=16):
                    brain.wm_push(line.lstrip("-*• "), context="assistant", salience=cfg.assistant_salience)
                    report.wm_pushed += 1
        else:
            brain.wm_push(assistant_text.strip(), context="assistant", salience=cfg.assistant_salience)
            report.wm_pushed += 1

    if cfg.flush:
        report.flushed = brain.turn_end(flush=True)
    else:
        report.flushed = brain.turn_end(flush=False)
    return report


class ReflexHook:
    """Cursor / hook-friendly wrapper: call after each agent turn."""

    def __init__(self, brain: Any, config: Optional[ReflexConfig] = None) -> None:
        self.brain = brain
        self.config = config or ReflexConfig()

    def after_turn(
        self,
        payload: dict[str, Any],
    ) -> ReflexReport:
        tools = [
            ToolTurnResult(
                name=str(t.get("name", "tool")),
                result=str(t.get("result", "")),
                uri=t.get("uri"),
            )
            for t in payload.get("tools", [])
        ]
        return reflex_ingest_turn(
            self.brain,
            user_text=payload.get("user"),
            assistant_text=payload.get("assistant"),
            tools=tools,
            config=self.config,
        )
