# Multi-agent project brains

FluctlightDB can act as a **hub-and-spoke project brain** in a monorepo: one shared project memory plus per-tool agent memories (Cursor, Claude Code, Codex), with structured handoffs between them.

## Quick start

From your repo root:

```bash
pip install "fluctlightdb[native,mcp]"
fluctlight-project init
```

This creates:

```
.fluctlight/
  config.yaml
  project/           # shared hub brain
  agents/
    cursor/
    claude/
    codex/
```

Optional scaffolds (default: all):

- **Cursor** — `.cursor/mcp.json`, `.cursor/hooks.json`, hook scripts
- **Claude Code** — `.claude/skills/fluctlight-memory/SKILL.md`, `CLAUDE.md` snippet
- **Codex** — `.fluctlight/codex.env.example`

Brains are appended to `.gitignore` by default (local-first). Commit them if your team wants shared memory in git.

## Python API

```python
from fluctlightdb import connect_project

pb = connect_project()  # auto-detects agent; walks up to find .fluctlight/
pb.remember("Use ruff for linting", scope="project", context="conventions")
pb.recall("linting conventions", scope="all")
pb.handoff("Refactored auth module", next_steps=["Add tests"], files=["src/auth/"])
print(pb.session_context())
```

### Agent identity

Set explicitly or auto-detect:

| Variable | Purpose |
|----------|---------|
| `FLUCTLIGHT_AGENT` | `cursor`, `claude`, `codex`, or custom name |
| `FLUCTLIGHT_SESSION_ID` | Stable session id for handoffs |
| `CURSOR_SESSION_ID` | Auto-detect Cursor |

Subdirectory context is inferred from cwd relative to project root (`session:cursor:apps/api`).

## Handoffs

`handoff()` stores JSON in the project brain with context `handoff:<agent>:<subdir>`. Other agents see recent handoffs via `recall_handoffs()` and `session_context()`.

## MCP server

After `fluctlight-project init`, Cursor loads:

```json
{
  "mcpServers": {
    "fluctlight": {
      "command": "python3",
      "args": ["-m", "fluctlightdb.mcp_server"],
      "env": { "FLUCTLIGHT_AGENT": "cursor" }
    }
  }
}
```

Tools: `fluctlight_recall`, `fluctlight_remember`, `fluctlight_handoff`, `fluctlight_session_context`, `fluctlight_status`.

Run manually: `python3 -m fluctlightdb.mcp_server`

## Cursor hooks

| Event | Behavior |
|-------|----------|
| `sessionStart` | Inject `session_context()` as `additional_context` |
| `sessionEnd` | Checkpoint brains |
| `stop` | Write a brief handoff (disable with `FLUCTLIGHT_SKIP_STOP_HANDOFF=1`) |

Hooks fail open if native is not installed.

## Concurrency

Writes use `fcntl` file locks (`.brain.lock`) per brain directory so MCP, hooks, and multiple processes can share a repo safely.

## CLI

```bash
fluctlight-project init [--name myapp] [--cursor] [--claude] [--codex] [--force]
fluctlight-project status
fluctlight-project context
```

## Example monorepo

See [examples/multi-agent-monorepo](../examples/multi-agent-monorepo/).

## Related

- [GETTING_STARTED.md](GETTING_STARTED.md)
- [PLATFORMS.md](PLATFORMS.md)
