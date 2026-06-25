"""Tests for multi-agent project brain scaffolding."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import threading
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
            self.assertTrue((root / ".fluctlight" / "codex.mcp.json").is_file())

    def test_init_team_sync(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = os.environ.copy()
            env["PYTHONPATH"] = str(ROOT)
            subprocess.run(
                [sys.executable, "-m", "fluctlightdb.cli", "init", str(root), "--team-sync"],
                env=env,
                check=True,
                capture_output=True,
            )
            gi = (root / ".gitignore").read_text(encoding="utf-8")
            self.assertIn(".fluctlight/agents/", gi)
            self.assertNotIn(".fluctlight/project/", gi)
            self.assertTrue((root / ".fluctlight" / "TEAM_SYNC.md").is_file())

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
                listed = pb.list_handoffs(limit=5)
                self.assertEqual(len(listed), 1)
                self.assertEqual(listed[0].summary, "done step one")
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


class TestHandoffIndex(unittest.TestCase):
    def test_list_filters(self) -> None:
        from fluctlightdb.handoff import Handoff
        from fluctlightdb.handoff_index import append_handoff, list_handoffs

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / ".fluctlight"
            root.mkdir()
            h1 = Handoff(agent="cursor", subdir="apps/api", summary="one", status="paused")
            h2 = Handoff(agent="claude", subdir="apps/web", summary="two", status="done")
            append_handoff(root, h1)
            append_handoff(root, h2)
            all_items = list_handoffs(root, limit=10)
            self.assertEqual(len(all_items), 2)
            cursor_only = list_handoffs(root, agent="cursor", limit=10)
            self.assertEqual(len(cursor_only), 1)
            self.assertEqual(cursor_only[0].summary, "one")


class TestLock(unittest.TestCase):
    def test_cross_process_style_lock(self) -> None:
        from fluctlightdb.lock import brain_write_lock

        with tempfile.TemporaryDirectory() as tmp:
            brain = Path(tmp) / "brain"
            brain.mkdir()
            acquired = threading.Event()
            release = threading.Event()

            def holder() -> None:
                with brain_write_lock(brain, timeout_s=5.0):
                    acquired.set()
                    release.wait(timeout=3.0)

            t = threading.Thread(target=holder)
            t.start()
            self.assertTrue(acquired.wait(timeout=2.0))
            with self.assertRaises(TimeoutError):
                with brain_write_lock(brain, timeout_s=0.2):
                    pass
            release.set()
            t.join(timeout=2.0)
            with brain_write_lock(brain, timeout_s=2.0):
                pass


class TestValidation(unittest.TestCase):
    def test_rejects_secrets(self) -> None:
        from fluctlightdb.validation import validate_content

        with self.assertRaises(ValueError):
            validate_content("token ghp_abcdefghijklmnopqrstuvwxyz1234567890")

    def test_max_length(self) -> None:
        from fluctlightdb.validation import validate_content

        with self.assertRaises(ValueError):
            validate_content("x" * 9000)


class TestPlatform(unittest.TestCase):
    def test_brain_lock_path_dir(self) -> None:
        from fluctlightdb.platform import brain_lock_path

        p = brain_lock_path("/tmp/mybrain")
        self.assertTrue(str(p).endswith(".brain.lock"))


    def test_build_inbox_html(self) -> None:
        from fluctlightdb.inbox_ui import build_inbox_html

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            env = os.environ.copy()
            env["PYTHONPATH"] = str(ROOT)
            subprocess.run(
                [sys.executable, "-m", "fluctlightdb.cli", "init", str(root), "--name", "ui-test"],
                env=env,
                check=True,
                capture_output=True,
            )
            html = build_inbox_html(root)
            self.assertIn("FluctlightDB project brain", html)
            self.assertIn("ui-test", html)


class TestDoctor(unittest.TestCase):
    def test_doctor_without_project(self) -> None:
        from fluctlightdb.doctor import run_doctor

        with tempfile.TemporaryDirectory() as tmp:
            old = os.getcwd()
            try:
                os.chdir(tmp)
                report = run_doctor()
                self.assertFalse(report.ok)
                names = [c.name for c in report.checks]
                self.assertIn("config", names)
            finally:
                os.chdir(old)


if __name__ == "__main__":
    unittest.main()
