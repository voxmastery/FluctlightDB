"""MCP server exposing FluctlightDB project brains to Cursor / Claude / Codex."""

from __future__ import annotations

import json
import os
from typing import Optional


def _connect():
    from .project import connect_project

    agent = os.environ.get("FLUCTLIGHT_AGENT", "auto")
    return connect_project(agent=agent)


def run() -> None:
    try:
        from mcp.server.fastmcp import FastMCP
    except ImportError as exc:
        raise SystemExit(
            "MCP support requires: pip install 'fluctlightdb[mcp]'\n"
            "Embedded brains also need: pip install 'fluctlightdb[native]'"
        ) from exc

    mcp = FastMCP("fluctlight")

    @mcp.tool()
    def fluctlight_status() -> str:
        """Project brain status: agent, subdir, recent handoffs."""
        return json.dumps(_connect().status(), indent=2)

    @mcp.tool()
    def fluctlight_recall(cue: str, scope: str = "all", limit: int = 12) -> str:
        """Recall memories by cue from project and/or agent brain."""
        payload = _connect().recall(cue, scope=scope, limit=limit)
        return json.dumps(payload, indent=2)

    @mcp.tool()
    def fluctlight_remember(
        content: str,
        scope: str = "agent",
        context: str = "session",
        salience: float = 0.55,
    ) -> str:
        """Store a memory in the agent or shared project brain."""
        payload = _connect().remember(
            content,
            scope=scope,
            context=context,
            salience=salience,
        )
        return json.dumps(payload, indent=2)

    @mcp.tool()
    def fluctlight_handoff(
        summary: str,
        status: str = "paused",
        next_steps: Optional[list[str]] = None,
        files: Optional[list[str]] = None,
    ) -> str:
        """Write a structured handoff for other agents (Cursor, Claude, Codex)."""
        h = _connect().handoff(
            summary,
            status=status,
            next_steps=next_steps,
            files=files,
        )
        return json.dumps(
            {
                "handoff_id": h.handoff_id,
                "agent": h.agent,
                "subdir": h.subdir,
                "status": h.status,
                "summary": h.summary,
            },
            indent=2,
        )

    @mcp.tool()
    def fluctlight_list_handoffs(
        agent: Optional[str] = None,
        subdir: Optional[str] = None,
        status: Optional[str] = None,
        since: Optional[str] = None,
        limit: int = 20,
    ) -> str:
        """List handoffs from the deterministic inbox (filter by agent, subdir, status)."""
        pb = _connect()
        items = pb.list_handoffs(
            agent=agent,
            subdir=subdir,
            status=status,
            since=since,
            limit=limit,
        )
        payload = [
            {
                "handoff_id": h.handoff_id,
                "agent": h.agent,
                "subdir": h.subdir,
                "status": h.status,
                "summary": h.summary,
                "next_steps": h.next_steps,
                "files": h.files,
                "created_at": h.created_at,
            }
            for h in items
        ]
        return json.dumps(payload, indent=2)

    @mcp.tool()
    def fluctlight_session_context(limit: int = 10) -> str:
        """Compact recalled context + handoffs for system prompt injection."""
        return _connect().session_context(limit=limit)

    mcp.run()


if __name__ == "__main__":
    run()
