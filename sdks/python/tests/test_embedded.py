"""Embedded production path — connect_embedded + WM recall before flush."""

from __future__ import annotations

import importlib.util
import os
import shutil
import stat
import tempfile
import unittest

_HAS_NATIVE = importlib.util.find_spec("fluctlightdb_native") is not None


def _hits(result: object) -> list[object]:
    if not isinstance(result, dict):
        return []
    return list(result.get("hits") or result.get("recalls") or [])


@unittest.skipUnless(_HAS_NATIVE, "fluctlightdb[native] not installed")
class TestEmbedded(unittest.TestCase):
    def test_connect_embedded_wm_recall_before_flush(self) -> None:
        from fluctlightdb import connect_embedded

        tmp = tempfile.mkdtemp(prefix="flct-embedded-")
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        brain_path = os.path.join(tmp, "agent-brain")

        brain = connect_embedded(brain_path)
        brain.turn_begin()
        brain.wm_push("User prefers dark mode", context="settings", salience=0.8)
        result = brain.recall("dark mode")
        self.assertTrue(
            _hits(result),
            f"embedded WM should recall before flush: {result!r}",
        )

    def test_secure_brain_directory_unix(self) -> None:
        if os.name == "nt":
            self.skipTest("chmod check is Unix-only")
        from fluctlightdb.brain import _secure_brain_directory

        tmp = tempfile.mkdtemp(prefix="flct-secure-")
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        brain_path = os.path.join(tmp, "nested", "brain")
        _secure_brain_directory(brain_path)
        parent_mode = stat.S_IMODE(os.stat(os.path.dirname(brain_path)).st_mode)
        self.assertEqual(parent_mode, 0o700)


if __name__ == "__main__":
    unittest.main()
