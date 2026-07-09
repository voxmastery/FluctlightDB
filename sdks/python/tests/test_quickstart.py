"""Guards documented copy-paste snippets — zero-config recall must not silently return empty."""

from __future__ import annotations

import importlib.util
import os
import shutil
import tempfile
import unittest
from typing import Any

_HAS_NATIVE = importlib.util.find_spec("fluctlightdb_native") is not None


def _hits(result: Any) -> list[Any]:
    if not isinstance(result, dict):
        return []
    return list(result.get("hits") or result.get("recalls") or [])


@unittest.skipUnless(_HAS_NATIVE, "fluctlightdb[native] not installed")
class TestReadmeQuickstart(unittest.TestCase):
    def test_readme_30_second_quickstart(self) -> None:
        from fluctlightdb import connect_agent

        tmp = tempfile.mkdtemp(prefix="flct-readme-")
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        brain_path = os.path.join(tmp, "my-agent-brain")

        brain = connect_agent(brain_path)
        brain.turn_begin()
        brain.wm_push("User prefers dark mode", context="settings", salience=0.8)
        brain.turn_end(flush=True)
        result = brain.recall("dark mode")

        hits = _hits(result)
        self.assertTrue(
            hits,
            f"README quickstart must return non-empty recall; got {result!r}",
        )
        joined = " ".join(
            str(h.get("content") or h.get("snippet") or h.get("episode", {}).get("content") or h)
            for h in hits
        ).lower()
        self.assertIn("dark", joined)

    def test_hub_paper_connect_snippet(self) -> None:
        from fluctlightdb import connect

        tmp = tempfile.mkdtemp(prefix="flct-paper-")
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        brain = connect(os.path.join(tmp, "agent-brain"))
        brain.experience("User prefers dark mode", context="settings", salience=0.8)
        result = brain.activate("dark mode")
        self.assertTrue(_hits(result), f"hub/paper README snippet failed: {result!r}")

    def test_sdks_python_readme_snippet(self) -> None:
        from fluctlightdb import connect

        tmp = tempfile.mkdtemp(prefix="flct-sdk-readme-")
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        brain = connect(os.path.join(tmp, "my-agent-brain"))
        brain.experience("User prefers dark mode", context="settings", salience=0.8)
        result = brain.activate("dark mode")
        self.assertTrue(_hits(result), f"sdks/python/README snippet failed: {result!r}")

    def test_embeddings_lexical_snippet(self) -> None:
        from fluctlightdb import connect_agent

        tmp = tempfile.mkdtemp(prefix="flct-embed-")
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        brain = connect_agent(os.path.join(tmp, "agent"))
        brain.experience(content="User prefers dark mode", context="settings", salience=0.8)
        result = brain.activate("dark mode")
        self.assertTrue(_hits(result), f"docs/EMBEDDINGS lexical snippet failed: {result!r}")

    def test_onboarding_wm_snippet(self) -> None:
        from fluctlightdb import connect_agent

        tmp = tempfile.mkdtemp(prefix="flct-onboard-")
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        brain = connect_agent()
        brain.turn_begin()
        brain.wm_push("Use pytest for tests", context="project", salience=0.7)
        brain.turn_end(flush=True)
        result = brain.recall("pytest")
        self.assertTrue(_hits(result), f"docs/ONBOARDING snippet failed: {result!r}")

    def test_wm_recall_before_flush(self) -> None:
        from fluctlightdb import connect_agent

        tmp = tempfile.mkdtemp(prefix="flct-wm-pre-flush-")
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        brain = connect_agent(os.path.join(tmp, "brain"))
        brain.turn_begin()
        brain.wm_push("User prefers dark mode", context="settings", salience=0.8)
        result = brain.recall("dark mode")
        self.assertTrue(
            _hits(result),
            f"WM should be searchable before turn_end flush: {result!r}",
        )

    def test_readme_paraphrase_needs_vector_or_lexical_cue(self) -> None:
        """Paraphrase without semantic_vector needs embedder or overlapping tokens."""
        from fluctlightdb import connect_agent

        tmp = tempfile.mkdtemp(prefix="flct-readme-para-")
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        brain = connect_agent(os.path.join(tmp, "brain"))
        brain.turn_begin()
        brain.wm_push("User prefers dark mode", context="settings", salience=0.8)
        brain.turn_end(flush=True)
        paraphrase = brain.recall("theme preference")
        keyword = brain.recall("dark mode")
        self.assertTrue(_hits(keyword), "lexical cue should hit")
        if not _hits(paraphrase):
            self.assertTrue(_hits(keyword))


if __name__ == "__main__":
    unittest.main()
