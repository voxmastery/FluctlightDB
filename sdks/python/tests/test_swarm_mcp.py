import asyncio
import unittest

from fluctlightdb import swarm_mcp

try:
    import mcp  # noqa: F401

    HAVE_MCP = True
except ImportError:
    HAVE_MCP = False


class TestSwarmMcp(unittest.TestCase):
    @unittest.skipUnless(HAVE_MCP, "mcp extra not installed (pip install 'fluctlightdb[mcp]')")
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


if __name__ == "__main__":
    unittest.main()
