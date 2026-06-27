# Multi-agent project brains

FluctlightDB acts as a **hub-and-spoke project brain** in a monorepo: one shared project memory plus per-tool agent memories (Cursor, Claude Code, Codex), with structured handoffs between them.

**Platforms:** Linux, macOS, Windows — see [PLATFORM_COMPAT.md](PLATFORM_COMPAT.md).

## Quick start

From your repo root:

```bash
pip install "fluctlightdb[native,mcp]"
fluctlight-project init
fluctlight-project doctor
```

This creates:

```
.fluctlight/
  config.yaml
  handoffs.jsonl      # deterministic handoff inbox
  project/            # shared hub brain
  agents/
    cursor/
    claude/
    codex/
```

Optional scaffolds (default: all):

- **Cursor** — `.cursor/mcp.json`, `.cursor/hooks.json`, hook scripts (`.cmd` on Windows)
- **Claude Code** — `.claude/settings.json` (MCP), skill, `CLAUDE.md` snippet
- **Codex** — `.fluctlight/codex.mcp.json`, env example

Brains are gitignored by default (local-first). Use `--team-sync` to commit the shared project hub.

## Python API

```python
from fluctlightdb import connect_project

pb = connect_project()  # auto-detects agent; walks up to find .fluctlight/
pb.remember("Use ruff for linting", scope="project", context="conventions")
pb.recall("linting conventions", scope="all")
pb.handoff("Refactored auth module", next_steps=["Add tests"], files=["src/auth/"])
print(pb.list_handoffs(agent="cursor"))
print(pb.session_context())
```

### Agent identity

| Variable | Purpose |
|----------|---------|
| `FLUCTLIGHT_AGENT` | `cursor`, `claude`, `codex`, or custom name |
| `FLUCTLIGHT_SESSION_ID` | Stable session id for handoffs |
| `CURSOR_SESSION_ID` | Auto-detect Cursor |

Subdirectory context is inferred from cwd relative to project root (`session:cursor:apps/api`).

## Handoff inbox

`handoff()` writes to:

1. Project brain (cue-recallable episode)
2. `.fluctlight/handoffs.jsonl` (deterministic inbox)

```python
pb.list_handoffs(agent="claude", subdir="apps/api", status="paused", limit=10)
pb.get_handoff("a4feade2")
```

CLI:

```bash
fluctlight-project handoffs --agent cursor --json
```

## MCP server

Tools: `fluctlight_recall`, `fluctlight_remember`, `fluctlight_handoff`, `fluctlight_list_handoffs`, `fluctlight_session_context`, `fluctlight_status`.

Run manually: `python -m fluctlightdb.mcp_server`

## Cursor hooks

| Event | Behavior |
|-------|----------|
| `sessionStart` | Inject `session_context()` as `additional_context` |
| `sessionEnd` | Checkpoint brains |
| `stop` | Git-aware handoff (diff stat, files, branch) |
| `postToolUse` (Write/Edit) | Track edited files in agent brain |

Disable stop handoff: `FLUCTLIGHT_SKIP_STOP_HANDOFF=1`

Hooks fail open if native is not installed.

## Concurrency

Writes use cross-platform file locks (`.brain.lock` per brain, `.handoffs.lock` for inbox). Safe across MCP, hooks, and multiple processes on Linux, macOS, and Windows.

## Serve vs embedded

Use **either** embedded `connect_project()` **or** `fluctlight-serve` on the same brain path — not both. If `FLUCTLIGHT_SERVE_URL` is set, `connect_project()` logs a warning.

## CLI

```bash
fluctlight-project init [--name myapp] [--team-sync] [--cursor] [--claude] [--codex] [--force]
fluctlight-project onboard
fluctlight-project doctor [--json]
fluctlight-project ui [--port 8787]
fluctlight-project sync pull|push|status
fluctlight-project handoffs [--agent X] [--subdir Y] [--json]
fluctlight-project status
fluctlight-project context
```

## VPS + local desktop

See **[VPS_DESKTOP.md](VPS_DESKTOP.md)** — Cursor CLI on VPS and Cursor on your laptop sharing one project brain (git sync or HTTP hub).

## Team sync

```bash
fluctlight-project init --team-sync
```

Commits shared project brain + handoffs; keeps per-agent spokes local. See `.fluctlight/TEAM_SYNC.md`.

## Safety

- Max content size: 8 KB default (`FLUCTLIGHT_MAX_CONTENT`)
- Secret patterns rejected unless `FLUCTLIGHT_ALLOW_SECRETS=1`

## Example monorepo

See [examples/multi-agent-monorepo](../examples/multi-agent-monorepo/).

## Related

- [PLATFORM_COMPAT.md](PLATFORM_COMPAT.md)
- [GETTING_STARTED.md](GETTING_STARTED.md)
- [PUBLISHING.md](PUBLISHING.md)
