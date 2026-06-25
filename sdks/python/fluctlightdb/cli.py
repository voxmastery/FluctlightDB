"""`fluctlight-project` — scaffold multi-agent project brains."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import stat
import sys
from pathlib import Path
from typing import Any, Optional

import yaml

from .platform import is_windows, python_for_mcp, python_mcp_args
from .project import CONFIG_FILE, FLUCTLIGHT_DIR, ProjectConfig

_TEMPLATES = Path(__file__).resolve().parent / "templates"


def _template_path(name: str) -> Path:
    return _TEMPLATES / name


def _render(text: str, **kwargs: str) -> str:
    for key, val in kwargs.items():
        text = text.replace(f"{{{{{key}}}}}", val)
    return text


def _mcp_render_kwargs(agent: str) -> dict[str, str]:
    args = python_mcp_args() + ["-m", "fluctlightdb.mcp_server"]
    cmd = args[0]
    mcp_args = args[1:] if len(args) > 1 else ["-m", "fluctlightdb.mcp_server"]
    return {
        "python_cmd": cmd,
        "python_args": json.dumps(mcp_args),
        "project_id": agent,
    }


def _write(path: Path, content: str, *, force: bool = False) -> bool:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists() and not force:
        return False
    path.write_text(content, encoding="utf-8")
    return True


def _copy_executable(src: Path, dest: Path, *, force: bool = False) -> bool:
    if dest.exists() and not force:
        return False
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dest)
    dest.chmod(dest.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    return True


def _default_config(project_id: str, *, team_sync: bool = False) -> dict[str, Any]:
    return {
        "version": 1,
        "project_id": project_id,
        "brains": {
            "project": "project",
            "agents": {
                "cursor": "agents/cursor",
                "claude": "agents/claude",
                "codex": "agents/codex",
            },
        },
        "serve": {"enabled": False},
        "defaults": {"agent": "auto"},
        "git": {"ignore_brains": not team_sync, "team_sync": team_sync},
    }


def _gitignore_snippet(*, team_sync: bool = False) -> str:
    if team_sync:
        return """
# FluctlightDB — team-sync: shared project brain is committed; agent spokes stay local
.fluctlight/agents/
.fluctlight/**/.brain.lock
.fluctlight/.handoffs.lock
""".strip() + "\n"
    return """
# FluctlightDB embedded brains (local agent memory; optional commit for team sync)
.fluctlight/project/
.fluctlight/agents/
.fluctlight/**/.brain.lock
.fluctlight/.handoffs.lock
""".strip() + "\n"


def cmd_init(args: argparse.Namespace) -> int:
    root = Path(args.path or os.getcwd()).resolve()
    project_id = args.name or root.name
    cfg_path = root / FLUCTLIGHT_DIR / CONFIG_FILE
    if cfg_path.is_file() and not args.force:
        print(f"Already initialized: {cfg_path}", file=sys.stderr)
        print("Use --force to overwrite config and re-scaffold integrations.", file=sys.stderr)
        return 1

    team_sync = bool(args.team_sync)
    cfg = _default_config(project_id, team_sync=team_sync)
    if args.agents:
        agents = [a.strip().lower() for a in args.agents.split(",") if a.strip()]
        cfg["brains"]["agents"] = {name: f"agents/{name}" for name in agents}

    fluct = root / FLUCTLIGHT_DIR
    (fluct / "project").mkdir(parents=True, exist_ok=True)
    for rel in cfg["brains"]["agents"].values():
        (fluct / rel).mkdir(parents=True, exist_ok=True)

    _write(cfg_path, yaml.safe_dump(cfg, sort_keys=False, default_flow_style=False), force=True)
    print(f"Created {cfg_path.relative_to(root)}")

    if team_sync:
        team_src = _template_path("TEAM_SYNC.md")
        team_dest = fluct / "TEAM_SYNC.md"
        if team_src.is_file() and _write(team_dest, team_src.read_text(encoding="utf-8"), force=args.force):
            print(f"Created {team_dest.relative_to(root)}")

    gitignore = root / ".gitignore"
    snippet = _gitignore_snippet(team_sync=team_sync)
    if args.gitignore:
        if gitignore.exists():
            body = gitignore.read_text(encoding="utf-8")
            if ".fluctlight/" not in body:
                gitignore.write_text(body.rstrip() + "\n\n" + snippet, encoding="utf-8")
                print(f"Appended FluctlightDB paths to {gitignore.relative_to(root)}")
        else:
            _write(gitignore, snippet)
            print(f"Created {gitignore.relative_to(root)}")

    scaffold = {
        "cursor": args.cursor or args.all,
        "claude": args.claude or args.all,
        "codex": args.codex or args.all,
    }
    if not any(scaffold.values()):
        scaffold = {"cursor": True, "claude": True, "codex": True}

    mcp_kwargs = _mcp_render_kwargs(project_id)
    created: list[str] = []
    if scaffold["cursor"]:
        created.extend(_scaffold_cursor(root, project_id, mcp_kwargs=mcp_kwargs, force=args.force))
    if scaffold["claude"]:
        created.extend(_scaffold_claude(root, project_id, mcp_kwargs=mcp_kwargs, force=args.force))
    if scaffold["codex"]:
        created.extend(_scaffold_codex(root, project_id, mcp_kwargs=mcp_kwargs, force=args.force))

    for rel in created:
        print(f"  + {rel}")
    print("\nNext: pip install 'fluctlightdb[native,mcp]'")
    print("  fluctlight-project doctor")
    return 0


def _hook_command(name: str) -> str:
    ext = ".cmd" if is_windows() else ".py"
    return f".cursor/hooks/{name}{ext}"


def _scaffold_cursor(root: Path, project_id: str, *, mcp_kwargs: dict[str, str], force: bool) -> list[str]:
    out: list[str] = []
    render = {**mcp_kwargs, "project_id": project_id}

    mcp_src = _template_path("cursor/mcp.json")
    mcp_dest = root / ".cursor" / "mcp.json"
    if mcp_src.is_file():
        text = _render(mcp_src.read_text(encoding="utf-8"), **render)
        if _write(mcp_dest, text, force=force):
            out.append(str(mcp_dest.relative_to(root)))

    hooks_src = _template_path("cursor/hooks.json")
    hooks_dest = root / ".cursor" / "hooks.json"
    if hooks_src.is_file():
        hook_render = {
            "session_start_hook": _hook_command("session_start"),
            "session_end_hook": _hook_command("session_end"),
            "stop_handoff_hook": _hook_command("stop_handoff"),
            "track_files_hook": _hook_command("track_files"),
        }
        text = _render(hooks_src.read_text(encoding="utf-8"), **hook_render)
        if _write(hooks_dest, text, force=force):
            out.append(str(hooks_dest.relative_to(root)))

    for script in ("session_start.py", "session_end.py", "stop_handoff.py", "track_files.py"):
        src = _template_path(f"cursor/hooks/{script}")
        dest = root / ".cursor" / "hooks" / script
        if src.is_file() and _copy_executable(src, dest, force=force):
            out.append(str(dest.relative_to(root)))

    if is_windows():
        for script in ("session_start.cmd", "session_end.cmd", "stop_handoff.cmd", "track_files.cmd"):
            src = _template_path(f"cursor/hooks/{script}")
            dest = root / ".cursor" / "hooks" / script
            if src.is_file() and _write(dest, src.read_text(encoding="utf-8"), force=force):
                out.append(str(dest.relative_to(root)))
    return out


def _scaffold_claude(root: Path, project_id: str, *, mcp_kwargs: dict[str, str], force: bool) -> list[str]:
    out: list[str] = []
    render = {**mcp_kwargs, "project_id": project_id}

    settings_src = _template_path("claude/settings.json")
    settings_dest = root / ".claude" / "settings.json"
    if settings_src.is_file():
        text = _render(settings_src.read_text(encoding="utf-8"), **render)
        if settings_dest.exists() and not force:
            try:
                existing = json.loads(settings_dest.read_text(encoding="utf-8"))
                incoming = json.loads(text)
                servers = existing.setdefault("mcpServers", {})
                servers.update(incoming.get("mcpServers", {}))
                settings_dest.write_text(json.dumps(existing, indent=2) + "\n", encoding="utf-8")
                out.append(f"{settings_dest.relative_to(root)} (merged)")
            except json.JSONDecodeError:
                if _write(settings_dest, text, force=force):
                    out.append(str(settings_dest.relative_to(root)))
        elif _write(settings_dest, text, force=force):
            out.append(str(settings_dest.relative_to(root)))

    skill_src = _template_path("claude/skills/fluctlight-memory/SKILL.md")
    skill_dest = root / ".claude" / "skills" / "fluctlight-memory" / "SKILL.md"
    if skill_src.is_file():
        text = _render(skill_src.read_text(encoding="utf-8"), project_id=project_id)
        if _write(skill_dest, text, force=force):
            out.append(str(skill_dest.relative_to(root)))

    snippet_src = _template_path("claude/CLAUDE.snippet.md")
    snippet_dest = root / "CLAUDE.md"
    if snippet_src.is_file():
        snippet = _render(snippet_src.read_text(encoding="utf-8"), project_id=project_id)
        if snippet_dest.exists() and not force:
            body = snippet_dest.read_text(encoding="utf-8")
            if "FluctlightDB" not in body:
                snippet_dest.write_text(body.rstrip() + "\n\n" + snippet, encoding="utf-8")
                out.append(f"{snippet_dest.relative_to(root)} (appended)")
        elif _write(snippet_dest, snippet, force=force):
            out.append(str(snippet_dest.relative_to(root)))
    return out


def _scaffold_codex(root: Path, project_id: str, *, mcp_kwargs: dict[str, str], force: bool) -> list[str]:
    out: list[str] = []
    render = {**mcp_kwargs, "project_id": project_id}

    mcp_src = _template_path("codex/mcp.json")
    mcp_dest = root / ".fluctlight" / "codex.mcp.json"
    if mcp_src.is_file():
        text = _render(mcp_src.read_text(encoding="utf-8"), **render)
        if _write(mcp_dest, text, force=force):
            out.append(str(mcp_dest.relative_to(root)))

    env_src = _template_path("codex/fluctlight.env.example")
    env_dest = root / ".fluctlight" / "codex.env.example"
    if env_src.is_file():
        text = _render(env_src.read_text(encoding="utf-8"), project_id=project_id)
        if _write(env_dest, text, force=force):
            out.append(str(env_dest.relative_to(root)))
    return out


def cmd_status(args: argparse.Namespace) -> int:
    from .project import connect_project

    try:
        pb = connect_project(start=args.path, agent=args.agent)
        print(json.dumps(pb.status(), indent=2))
        return 0
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        return 1


def cmd_context(args: argparse.Namespace) -> int:
    from .project import connect_project

    try:
        pb = connect_project(start=args.path, agent=args.agent)
        print(pb.session_context(limit=args.limit))
        return 0
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        return 1


def cmd_handoffs(args: argparse.Namespace) -> int:
    from .project import connect_project

    try:
        pb = connect_project(start=args.path, agent=args.agent)
        items = pb.list_handoffs(
            agent=args.agent_filter,
            subdir=args.subdir,
            status=args.status,
            since=args.since,
            limit=args.limit,
        )
        if args.json:
            payload = [
                {
                    "handoff_id": h.handoff_id,
                    "agent": h.agent,
                    "subdir": h.subdir,
                    "status": h.status,
                    "summary": h.summary,
                    "next_steps": h.next_steps,
                    "files": h.files,
                    "created_at": h.created_at,
                }
                for h in items
            ]
            print(json.dumps(payload, indent=2))
        else:
            for h in items:
                print(h.format_brief())
                print()
        return 0
    except Exception as exc:
        print(str(exc), file=sys.stderr)
        return 1


def cmd_doctor(args: argparse.Namespace) -> int:
    from .doctor import print_doctor, run_doctor

    report = run_doctor(start=args.path)
    print_doctor(report, as_json=args.json)
    return 0 if report.ok else 1


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="fluctlight-project",
        description="Scaffold and inspect FluctlightDB multi-agent project brains.",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    init = sub.add_parser("init", help="Create .fluctlight/ hub + agent spokes and integrations")
    init.add_argument("path", nargs="?", help="Project root (default: cwd)")
    init.add_argument("--name", help="project_id in config (default: directory name)")
    init.add_argument("--agents", help="Comma-separated agent names (default: cursor,claude,codex)")
    init.add_argument("--cursor", action="store_true", help="Scaffold Cursor MCP + hooks")
    init.add_argument("--claude", action="store_true", help="Scaffold Claude skill + MCP settings")
    init.add_argument("--codex", action="store_true", help="Scaffold Codex MCP + env example")
    init.add_argument("--all", action="store_true", help="Scaffold all integrations (default when none set)")
    init.add_argument("--team-sync", action="store_true", help="Commit shared project brain; keep agent spokes local")
    init.add_argument("--gitignore", action="store_true", default=True, help="Append .fluctlight/ to .gitignore")
    init.add_argument("--no-gitignore", action="store_false", dest="gitignore")
    init.add_argument("--force", action="store_true", help="Overwrite existing scaffold files")
    init.set_defaults(func=cmd_init)

    status = sub.add_parser("status", help="Print project brain status as JSON")
    status.add_argument("path", nargs="?", help="Start directory for project discovery")
    status.add_argument("--agent", help="Agent name (cursor, claude, codex, …)")
    status.set_defaults(func=cmd_status)

    ctx = sub.add_parser("context", help="Print session context block (for hooks / prompts)")
    ctx.add_argument("path", nargs="?", help="Start directory for project discovery")
    ctx.add_argument("--agent", help="Agent name")
    ctx.add_argument("--limit", type=int, default=10)
    ctx.set_defaults(func=cmd_context)

    handoffs = sub.add_parser("handoffs", help="List handoffs from deterministic inbox index")
    handoffs.add_argument("path", nargs="?", help="Start directory for project discovery")
    handoffs.add_argument("--agent", dest="agent_filter", help="Filter by agent name")
    handoffs.add_argument("--subdir", help="Filter by subdir")
    handoffs.add_argument("--status", help="Filter by status (paused, done, blocked)")
    handoffs.add_argument("--since", help="ISO timestamp — only handoffs after this time")
    handoffs.add_argument("--limit", type=int, default=20)
    handoffs.add_argument("--json", action="store_true", help="Output JSON")
    handoffs.set_defaults(func=cmd_handoffs)

    doctor = sub.add_parser("doctor", help="Check native, MCP, config, and lock health")
    doctor.add_argument("path", nargs="?", help="Start directory for project discovery")
    doctor.add_argument("--json", action="store_true", help="Output JSON")
    doctor.set_defaults(func=cmd_doctor)

    return parser


def main(argv: Optional[list[str]] = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    return int(args.func(args))


if __name__ == "__main__":
    raise SystemExit(main())
