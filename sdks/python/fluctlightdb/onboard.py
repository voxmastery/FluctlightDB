"""Interactive onboarding for FluctlightDB multi-agent setup."""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path


def run_onboard(*, path: Path | None = None) -> int:
    root = path or Path.cwd()
    print("FluctlightDB onboarding")
    print("=" * 40)
    print("\n1. Install:")
    print('   pip install "fluctlightdb[native,mcp]"')
    print("\n2. Initialize project brain (monorepo root):")
    print("   fluctlight-project init --team-sync")
    print("\n3. Verify:")
    print("   fluctlight-project doctor")
    print("\n4. Open handoff inbox UI:")
    print("   fluctlight-project ui")
    print("\n5. VPS + local desktop? See docs/VPS_DESKTOP.md")
    print("   - Git sync: fluctlight-project sync pull / sync push")
    print("   - Live hub: FLUCTLIGHT_HUB_URL=http://your-vps:8792")
    print("\n6. Cursor / Claude / Codex MCP is configured by init.")
    print("   Rules in .cursor/rules/fluctlight.mdc enforce memory use.")
    print()

    if sys.stdin.isatty():
        ans = input("Run init in current directory now? [y/N] ").strip().lower()
        if ans in ("y", "yes"):
            subprocess.run(
                [sys.executable, "-m", "fluctlightdb.cli", "init", str(root), "--team-sync"],
                check=False,
            )
            subprocess.run(
                [sys.executable, "-m", "fluctlightdb.cli", "doctor"],
                cwd=root,
                check=False,
            )
    return 0
