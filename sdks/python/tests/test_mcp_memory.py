"""MCP memory_remember is visible to memory_recall on a fresh connection."""

from __future__ import annotations

import importlib.util
import json
import os
import shutil
import tempfile
import unittest

_HAS_NATIVE = importlib.util.find_spec("fluctlightdb_native") is not None


@unittest.skipUnless(_HAS_NATIVE, "fluctlightdb[native] not installed")
class TestMcpMemoryDurability(unittest.TestCase):
    def test_remember_then_recall_from_fresh_connection(self) -> None:
        from fluctlightdb.mcp_server import memory_recall, memory_remember

        tmp = tempfile.mkdtemp(prefix="flct-mcp-")
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        brain_path = os.path.join(tmp, "agent-brain")
        previous = os.environ.get("FLUCTLIGHT_BRAIN_PATH")
        os.environ["FLUCTLIGHT_BRAIN_PATH"] = brain_path
        self.addCleanup(self._restore_env, previous)

        stored = json.loads(memory_remember("project codename is blue-heron", context="mcp"))
        self.assertTrue(stored.get("stored"))

        # memory_recall opens its own brain; the item must already be checkpointed.
        recalled = json.loads(memory_recall("blue-heron"))
        hits = recalled.get("hits") or recalled.get("recalls") or []
        contents = " ".join(
            str(hit.get("content") or hit.get("snippet") or "") for hit in hits if isinstance(hit, dict)
        )
        self.assertIn("blue-heron", contents, recalled)

    @staticmethod
    def _restore_env(previous: str | None) -> None:
        if previous is None:
            os.environ.pop("FLUCTLIGHT_BRAIN_PATH", None)
        else:
            os.environ["FLUCTLIGHT_BRAIN_PATH"] = previous


if __name__ == "__main__":
    unittest.main()
