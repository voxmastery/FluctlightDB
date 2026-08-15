"""Swarm MCP server wiring — guarded on the optional `mcp` extra.

`swarm_mcp.build_server()` imports the MCP SDK, which ships in the optional
`fluctlightdb[mcp]` extra. The CI `python-sdk` job installs base dependencies only
(`pip install -e ./sdks/python`), so an unguarded call raises
`RuntimeError: Install with: pip install 'fluctlightdb[mcp]'` and fails the job on every
OS rather than skipping. The rest of this suite already guards optional surfaces the same
way — see `_HAS_NATIVE` in `test_embedded.py` / `test_quickstart.py`.
"""

from __future__ import annotations

import asyncio
import importlib.util
import unittest

from fluctlightdb import swarm_mcp

_HAS_MCP = importlib.util.find_spec("mcp") is not None


@unittest.skipUnless(_HAS_MCP, "fluctlightdb[mcp] not installed")
class TestSwarmMcp(unittest.TestCase):
    def test_build_server_supports_installed_mcp_sdk(self) -> None:
        build_server = getattr(swarm_mcp, "build_server", None)
        self.assertTrue(callable(build_server), "swarm_mcp must expose build_server")

        server = build_server()
        tools = asyncio.run(server.list_tools())
        names = {tool.name for tool in tools}

        self.assertEqual(
            names,
            {
                "fluctlight_swarm_begin",
                "fluctlight_swarm_claim_hook",
                "fluctlight_swarm_cite",
                "fluctlight_swarm_report_hook",
                "fluctlight_swarm_get",
            },
        )


class TestSwarmMcpModule(unittest.TestCase):
    """Runs without the extra: the module must import and expose its entry point even when
    the MCP SDK is absent, so a missing optional dependency degrades to a clear error at
    call time rather than an import failure."""

    def test_module_exposes_build_server_without_mcp_installed(self) -> None:
        self.assertTrue(callable(getattr(swarm_mcp, "build_server", None)))


if __name__ == "__main__":
    unittest.main()
