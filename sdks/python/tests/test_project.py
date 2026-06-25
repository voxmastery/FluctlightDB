"""Tests for multi-agent project brain scaffolding."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))


class TestProjectInit(unittest.TestCase):
    def test_init_creates_config_and_scaffold(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = os.environ.copy()
            env["PYTHONPATH"] = str(ROOT)
            proc = subprocess.run(
                [sys.executable, "-m", "fluctlightdb.cli", "init", str(root), "--name", "testproj"],
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(proc.returncode, 0, proc.stderr or proc.stdout)
            cfg = root / ".fluctlight" / "config.yaml"
            self.assertTrue(cfg.is_file())
            self.assertTrue((root / ".fluctlight" / "project").is_dir())
            self.assertTrue((root / ".cursor" / "mcp.json").is_file())
            self.assertTrue((root / ".cursor" / "hooks.json").is_file())
            self.assertTrue((root / ".claude" / "skills" / "fluctlight-memory" / "SKILL.md").is_file())

    def test_find_project_root(self) -> None:
        from fluctlightdb.project import find_project_root

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / ".fluctlight").mkdir()
            (root / ".fluctlight" / "config.yaml").write_text("version: 1\nproject_id: x\n", encoding="utf-8")
            sub = root / "apps" / "api"
            sub.mkdir(parents=True)
            found = find_project_root(sub)
            self.assertEqual(found, root.resolve())

    @unittest.skipUnless(
        __import__("importlib").util.find_spec("fluctlightdb_native") is not None,  # type: ignore[attr-defined]
        "native extension not installed",
    )
    def test_connect_and_handoff(self) -> None:
        from fluctlightdb import connect_project

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = os.environ.copy()
            env["PYTHONPATH"] = str(ROOT)
            subprocess.run(
                [sys.executable, "-m", "fluctlightdb.cli", "init", str(root), "--name", "handoff-test"],
                env=env,
                check=True,
                capture_output=True,
            )
            sub = root / "pkg"
            sub.mkdir()
            old = os.getcwd()
            try:
                os.chdir(sub)
                os.environ["FLUCTLIGHT_AGENT"] = "cursor"
                pb = connect_project()
                pb.remember("test convention", scope="project", context="conventions")
                pb.handoff("done step one", next_steps=["step two"], files=["pkg/a.py"])
                status = pb.status()
                self.assertEqual(status["project_id"], "handoff-test")
                self.assertTrue(status["handoffs"])
            finally:
                os.chdir(old)


class TestHandoff(unittest.TestCase):
    def test_roundtrip(self) -> None:
        from fluctlightdb.handoff import Handoff

        h = Handoff(agent="cursor", subdir="apps/api", summary="ship it", next_steps=["test"])
        h2 = Handoff.from_content(h.to_content())
        self.assertEqual(h2.agent, "cursor")
        self.assertEqual(h2.summary, "ship it")
        self.assertEqual(h2.next_steps, ["test"])
        self.assertTrue(h.context_key().startswith("handoff:"))


if __name__ == "__main__":
    unittest.main()
