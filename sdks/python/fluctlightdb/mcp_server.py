"""MCP server exposing FluctlightDB project brains to Cursor / Claude / Codex."""

from __future__ import annotations

import json
import os
from typing import Optional


def _connect_agent():
    from .brain import connect_agent
    from .project import find_project_root

    path = os.environ.get("FLUCTLIGHT_BRAIN_PATH")
    if not path:
        root = find_project_root()
        agent = os.environ.get("FLUCTLIGHT_AGENT", "cursor")
        candidate = root / ".fluctlight" / "agents" / agent
        if candidate.is_dir():
            path = str(candidate)
    return connect_agent(path, retain_days=30)


def _connect_project():
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

    mcp = FastMCP("fluctlight-memory")

    # --- Standard memory MCP tools (agent brain) ---

    @mcp.tool()
    def memory_remember(
        content: str,
        context: str = "mcp",
        salience: float = 0.6,
    ) -> str:
        """Store a durable memory in the agent brain."""
        brain = _connect_agent()
        brain.turn_begin()
        brain.wm_push(content, context=context, salience=salience)
        report = brain.turn_end(flush=True)
        return json.dumps({"stored": True, "flush": report}, indent=2)

    @mcp.tool()
    def memory_recall(cue: str, limit: int = 8, mode: str = "auto") -> str:
        """Cue-driven recall from episodic + corpus lanes."""
        brain = _connect_agent()
        return json.dumps(brain.recall(cue, mode=mode, limit=limit), indent=2)

    @mcp.tool()
    def memory_resolve(cue: str) -> str:
        """Pick the trusted fact when memories disagree (conflict lattice)."""
        brain = _connect_agent()
        return json.dumps(brain.resolve(cue), indent=2)

    @mcp.tool()
    def memory_consolidate() -> str:
        """Run sleep: flush WM, CHORUS collapse, retention prune."""
        brain = _connect_agent()
        return json.dumps(brain.consolidate(), indent=2)

    @mcp.tool()
    def memory_observe_tool(
        tool_name: str,
        result: str,
        uri: Optional[str] = None,
    ) -> str:
        """Ingest MCP/tool output with ToolGrounded provenance."""
        brain = _connect_agent()
        return json.dumps(brain.observe_tool(tool_name, result, uri=uri), indent=2)

    # --- Project brain + handoffs (multi-agent) ---

    @mcp.tool()
    def fluctlight_status() -> str:
        """Project brain status: agent, subdir, recent handoffs."""
        return json.dumps(_connect_project().status(), indent=2)

    @mcp.tool()
    def fluctlight_recall(cue: str, scope: str = "all", limit: int = 12) -> str:
        """Recall memories by cue from project and/or agent brain."""
        payload = _connect_project().recall(cue, scope=scope, limit=limit)
        return json.dumps(payload, indent=2)

    @mcp.tool()
    def fluctlight_remember(
        content: str,
        scope: str = "agent",
        context: str = "session",
        salience: float = 0.55,
    ) -> str:
        """Store a memory in the agent or shared project brain."""
        payload = _connect_project().remember(
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
        h = _connect_project().handoff(
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
        pb = _connect_project()
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
        return _connect_project().session_context(limit=limit)

    mcp.run()


if __name__ == "__main__":
    run()
