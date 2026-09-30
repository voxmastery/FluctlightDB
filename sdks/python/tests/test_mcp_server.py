"""mcp extra stays on FastMCP, which registers the memory tools."""

from __future__ import annotations

import asyncio
import importlib.util
import re
import unittest
from pathlib import Path

_HAS_MCP = importlib.util.find_spec("mcp.server.fastmcp") is not None

_MEMORY_TOOLS = {
    "memory_remember",
    "memory_recall",
    "memory_resolve",
    "memory_consolidate",
    "memory_observe_tool",
    "fluctlight_status",
    "fluctlight_recall",
    "fluctlight_remember",
    "fluctlight_handoff",
    "fluctlight_list_handoffs",
    "fluctlight_session_context",
}


class TestMcpExtra(unittest.TestCase):
    def test_extra_pins_mcp_below_2(self) -> None:
        pyproject = Path(__file__).resolve().parents[1] / "pyproject.toml"
        text = pyproject.read_text(encoding="utf-8")
        match = re.search(r"^mcp\s*=\s*\[(.*?)\]", text, re.M)
        self.assertIsNotNone(match, "fluctlightdb[mcp] extra missing")
        body = match.group(1)
        spec = re.search(r"mcp([^\"']+)", body)
        self.assertIsNotNone(spec, body)
        requirement = "mcp" + spec.group(1).strip().rstrip(",")
        try:
            from packaging.requirements import Requirement
        except ImportError:
            self.assertIn(">=1.0", requirement)
            self.assertIn("<2", requirement)
            return
        parsed = Requirement(requirement)
        self.assertTrue(parsed.specifier.contains("1.0.0"))
        self.assertTrue(parsed.specifier.contains("1.30.0"))
        self.assertFalse(parsed.specifier.contains("2.0.0"))
        self.assertFalse(parsed.specifier.contains("2.1.0"))

    @unittest.skipUnless(_HAS_MCP, "mcp extra not installed (pip install 'fluctlightdb[mcp]')")
    def test_fastmcp_registers_eleven_tools(self) -> None:
        from mcp.server.fastmcp import FastMCP

        from fluctlightdb.mcp_server import build_server

        server = build_server()
        self.assertIsInstance(server, FastMCP)
        tools = asyncio.run(server.list_tools())
        names = {tool.name for tool in tools}
        self.assertEqual(names, _MEMORY_TOOLS)


if __name__ == "__main__":
    unittest.main()
