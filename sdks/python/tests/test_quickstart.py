"""Guards the verbatim README quickstart — zero-config connect_agent + wm_push + recall."""

from __future__ import annotations

import os
import shutil
import tempfile
import unittest


class TestReadmeQuickstart(unittest.TestCase):
    def test_readme_30_second_quickstart(self) -> None:
        try:
            from fluctlightdb import connect_agent
        except ImportError:
            self.skipTest("fluctlightdb[native] not installed")

        tmp = tempfile.mkdtemp(prefix="flct-readme-")
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        brain_path = os.path.join(tmp, "my-agent-brain")

        # Verbatim README quickstart (path adjusted to temp dir).
        brain = connect_agent(brain_path)
        brain.turn_begin()
        brain.wm_push("User prefers dark mode", context="settings", salience=0.8)
        brain.turn_end(flush=True)
        result = brain.recall("dark mode")

        hits = result.get("hits") or []
        self.assertTrue(
            hits,
            f"README quickstart must return non-empty recall; got {result!r}",
        )
        joined = " ".join(
            str(h.get("content") or h.get("snippet") or h) for h in hits
        ).lower()
        self.assertIn(
            "dark",
            joined,
            f"expected 'dark mode' preference in recall payload: {hits!r}",
        )

    def test_readme_paraphrase_needs_vector_or_lexical_cue(self) -> None:
        """Documented behavior: paraphrase without semantic_vector needs embedder or lexical cue."""
        try:
            from fluctlightdb import connect_agent
        except ImportError:
            self.skipTest("fluctlightdb[native] not installed")

        tmp = tempfile.mkdtemp(prefix="flct-readme-para-")
        self.addCleanup(shutil.rmtree, tmp, ignore_errors=True)
        brain = connect_agent(os.path.join(tmp, "brain"))
        brain.turn_begin()
        brain.wm_push("User prefers dark mode", context="settings", salience=0.8)
        brain.turn_end(flush=True)
        paraphrase = brain.recall("theme preference")
        keyword = brain.recall("dark mode")
        self.assertTrue(keyword.get("hits"), "lexical cue should hit")
        # Paraphrase without vectors may miss — not a silent storage failure.
        if not paraphrase.get("hits"):
            self.assertTrue(keyword.get("hits"))


if __name__ == "__main__":
    unittest.main()
