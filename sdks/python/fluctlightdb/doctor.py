"""Health checks for FluctlightDB project brain setup."""

from __future__ import annotations

import importlib.util
import json
import os
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Optional

from .lock import brain_write_lock
from .platform import brain_lock_path, is_windows, python_for_mcp, python_mcp_args
from .project import CONFIG_FILE, FLUCTLIGHT_DIR, find_project_root


@dataclass
class CheckResult:
    name: str
    ok: bool
    message: str
    hint: str = ""


@dataclass
class DoctorReport:
    checks: list[CheckResult] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        return all(c.ok for c in self.checks)

    def to_dict(self) -> dict[str, Any]:
        return {
            "ok": self.ok,
            "platform": sys.platform,
            "python": sys.executable,
            "python_for_mcp": python_for_mcp(),
            "python_mcp_args": python_mcp_args(),
            "checks": [
                {"name": c.name, "ok": c.ok, "message": c.message, "hint": c.hint}
                for c in self.checks
            ],
            "warnings": self.warnings,
        }


def _check_native() -> CheckResult:
    spec = importlib.util.find_spec("fluctlightdb_native")
    if spec is None:
        return CheckResult(
            "native",
            False,
            "fluctlightdb-native not installed",
            "pip install 'fluctlightdb[native]'",
        )
    try:
        import fluctlightdb_native as native  # type: ignore

        ver = getattr(native, "__version__", "unknown")
        return CheckResult("native", True, f"fluctlightdb-native {ver}")
    except ImportError as exc:
        return CheckResult("native", False, str(exc), "pip install 'fluctlightdb[native]'")


def _check_mcp() -> CheckResult:
    if importlib.util.find_spec("mcp.server.fastmcp") is None:
        return CheckResult(
            "mcp",
            False,
            "MCP extra not installed",
            "pip install 'fluctlightdb[mcp]'",
        )
    return CheckResult("mcp", True, "mcp package available")


def _check_config(start: Optional[Path]) -> CheckResult:
    try:
        root = find_project_root(start)
        cfg = root / FLUCTLIGHT_DIR / CONFIG_FILE
        return CheckResult("config", True, f"found {cfg.relative_to(root)}")
    except FileNotFoundError as exc:
        return CheckResult("config", False, str(exc), "fluctlight-project init")


def _check_lock(start: Optional[Path]) -> CheckResult:
    try:
        root = find_project_root(start)
        brain = root / FLUCTLIGHT_DIR / "project"
        brain.mkdir(parents=True, exist_ok=True)
        lock = brain_lock_path(brain)
        with brain_write_lock(brain, timeout_s=5.0):
            pass
        return CheckResult("lock", True, f"writable lock at {lock.name}")
    except Exception as exc:
        hint = "On Windows, close other agents using the same brain directory."
        if is_windows():
            hint += " Ensure no fluctlight-serve holds the lock."
        return CheckResult("lock", False, str(exc), hint)


def _check_serve_warning() -> Optional[str]:
    if os.environ.get("FLUCTLIGHT_SERVE_URL", "").strip():
        return (
            "FLUCTLIGHT_SERVE_URL is set — embedded brains may contend with fluctlight-serve. "
            "See docs/MULTI_AGENT.md."
        )
    return None


def run_doctor(*, start: Optional[os.PathLike[str] | str] = None) -> DoctorReport:
    report = DoctorReport()
    start_path = Path(start) if start else None
    report.checks.append(_check_native())
    report.checks.append(_check_mcp())
    report.checks.append(_check_config(start_path))
    report.checks.append(_check_lock(start_path))
    warn = _check_serve_warning()
    if warn:
        report.warnings.append(warn)
    return report


def print_doctor(report: DoctorReport, *, as_json: bool = False) -> None:
    if as_json:
        print(json.dumps(report.to_dict(), indent=2))
        return
    print(f"FluctlightDB doctor — platform: {sys.platform}")
    print(f"  Python: {sys.executable}")
    print(f"  MCP command: {' '.join(python_mcp_args() + ['-m', 'fluctlightdb.mcp_server'])}")
    for check in report.checks:
        mark = "OK" if check.ok else "FAIL"
        print(f"  [{mark}] {check.name}: {check.message}")
        if not check.ok and check.hint:
            print(f"         hint: {check.hint}")
    for warn in report.warnings:
        print(f"  [WARN] {warn}")
    print("Overall:", "healthy" if report.ok else "issues found")
