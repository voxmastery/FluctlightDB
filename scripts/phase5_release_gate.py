#!/usr/bin/env python3
"""Run Phase 5 release gates and emit a machine-readable JSON report."""

import argparse
import json
import os
import platform
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

COMMANDS = [
    ("format", ["cargo", "fmt", "--all", "--", "--check"]),
    (
        "default_tests",
        ["cargo", "test", "-p", "fluctlightdb", "--", "--test-threads=1"],
    ),
    (
        "distributed_tests",
        [
            "cargo",
            "test",
            "-p",
            "fluctlightdb",
            "--features",
            "distributed",
            "--",
            "--test-threads=1",
        ],
    ),
    (
        "security_tests",
        [
            "cargo",
            "test",
            "-p",
            "fluctlightdb",
            "--test",
            "zz_security_review",
            "--",
            "--test-threads=1",
        ],
    ),
    (
        "hyper_100k",
        [
            "cargo",
            "test",
            "-p",
            "fluctlightdb",
            "--release",
            "--test",
            "async_serve_boundary",
            "deterministic_hyper_malformed_request_gate_100k",
            "--",
            "--ignored",
            "--exact",
            "--test-threads=1",
        ],
    ),
    (
        "leader_failover",
        [
            "cargo",
            "test",
            "-p",
            "fluctlightdb",
            "--features",
            "distributed",
            "--test",
            "control_distributed_cluster",
            "leader_kill_surviving_quorum_elects_and_commits",
            "--",
            "--ignored",
            "--exact",
            "--test-threads=1",
        ],
    ),
    (
        "clippy",
        [
            "cargo",
            "clippy",
            "-p",
            "fluctlightdb",
            "--features",
            "distributed",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    ),
    ("diff_check", ["git", "diff", "--check"]),
]

REQUIRED_CONFIGURATION = {
    "server_mode": ("FLUCTLIGHT_SERVER_MODE", "production"),
    "authentication_required": ("FLUCTLIGHT_REQUIRE_AUTH", "true"),
    "tls_terminated": ("FLUCTLIGHT_TLS_TERMINATED", "true"),
    "backup_verified": ("FLUCTLIGHT_BACKUP_VERIFIED", "true"),
    "restore_drill_verified": ("FLUCTLIGHT_RESTORE_DRILL_VERIFIED", "true"),
}


def configuration_results():
    results = {}
    for name, (variable, expected) in REQUIRED_CONFIGURATION.items():
        value = os.environ.get(variable, "")
        passed = bool(value) if expected is None else value.lower() == expected
        results[name] = {"passed": passed, "variable": variable}
    return results


def windows_gate():
    if platform.system() == "Windows":
        return {"passed": True, "source": "current-host"}
    report = os.environ.get("FLUCTLIGHT_WINDOWS_GATE_REPORT")
    if not report:
        return {
            "passed": False,
            "environment_only": True,
            "reason": "Windows junction/reparse gate was not run",
        }
    try:
        data = json.loads(Path(report).read_text(encoding="utf-8"))
        return {"passed": data.get("windows_reparse_gate") == "passed", "source": report}
    except (OSError, ValueError) as error:
        return {"passed": False, "source": report, "reason": str(error)}


def external_report(variable, expected_key):
    path = os.environ.get(variable)
    if not path:
        return {"passed": False, "reason": f"{variable} is not configured"}
    try:
        data = json.loads(Path(path).read_text(encoding="utf-8"))
        return {"passed": data.get(expected_key) is True, "source": path}
    except (OSError, ValueError) as error:
        return {"passed": False, "source": path, "reason": str(error)}


def control_plane_bootstrap_report():
    variable = "FLUCTLIGHT_CONTROL_BOOTSTRAP_REPORT"
    path = os.environ.get(variable)
    if not path:
        return {"passed": False, "reason": f"{variable} is not configured"}
    try:
        data = json.loads(Path(path).read_text(encoding="utf-8"))
        passed = (
            data.get("control_plane_credential_bootstrap") is True
            and data.get("bootstrap_reuse_rejected") is True
            and data.get("plaintext_secret_persisted") is False
            and isinstance(data.get("raft_revision"), int)
            and data["raft_revision"] > 0
        )
        return {"passed": passed, "source": path}
    except (OSError, ValueError) as error:
        return {"passed": False, "source": path, "reason": str(error)}


def run_command(name, command, timeout):
    started = time.monotonic()
    try:
        completed = subprocess.run(
            command,
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=timeout,
            check=False,
        )
        return {
            "passed": completed.returncode == 0,
            "exit_code": completed.returncode,
            "duration_seconds": round(time.monotonic() - started, 3),
            "command": command,
            "output_tail": completed.stdout[-8000:],
        }
    except subprocess.TimeoutExpired as error:
        return {
            "passed": False,
            "timed_out": True,
            "duration_seconds": round(time.monotonic() - started, 3),
            "command": command,
            "output_tail": (error.stdout or "")[-8000:],
        }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--report", default="target/phase5-release-report.json")
    parser.add_argument("--timeout-seconds", type=int, default=3600)
    parser.add_argument("--quick", action="store_true", help="skip long gates; readiness stays false")
    args = parser.parse_args()

    selected = COMMANDS
    if args.quick:
        selected = [
            command
            for command in COMMANDS
            if command[0] in {"format", "security_tests", "diff_check"}
        ]
    checks = {
        name: run_command(name, command, args.timeout_seconds)
        for name, command in selected
    }
    if args.quick:
        for name, command in COMMANDS:
            checks.setdefault(
                name,
                {"passed": False, "skipped": True, "command": command},
            )

    report = {
        "schema_version": 1,
        "phase": 5,
        "host": {"system": platform.system(), "release": platform.release()},
        "configuration": configuration_results(),
        "checks": checks,
        "windows_reparse_gate": windows_gate(),
        "load_gate": external_report("FLUCTLIGHT_LOAD_GATE_REPORT", "passed"),
        "control_plane_credential_bootstrap": control_plane_bootstrap_report(),
    }
    report["production_ready"] = (
        all(item["passed"] for item in report["configuration"].values())
        and all(item["passed"] for item in checks.values())
        and report["windows_reparse_gate"]["passed"]
        and report["load_gate"]["passed"]
        and report["control_plane_credential_bootstrap"]["passed"]
    )
    destination = ROOT / args.report
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report["production_ready"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
