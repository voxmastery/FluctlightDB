# Multi-agent monorepo example

Minimal layout after `fluctlight-project init`:

```
my-monorepo/
  .fluctlight/
    config.yaml
    project/
    agents/cursor|claude|codex/
  apps/
    api/
    web/
  .cursor/          # MCP + hooks (Cursor)
  .claude/skills/   # fluctlight-memory skill (Claude Code)
```

## Init

```bash
cd examples/multi-agent-monorepo
pip install "fluctlightdb[native,mcp]"
fluctlight-project init --name demo-monorepo
```

## Connect from a subfolder

```bash
cd apps/api
python3 -c "
from fluctlightdb import connect_project
pb = connect_project()
pb.remember('API module uses FastAPI', scope='project', context='architecture')
print(pb.status())
"
```

## Handoff between tools

```python
from fluctlightdb import connect_project

# Cursor agent pauses work
cursor = connect_project(agent="cursor")
cursor.handoff(
    "Implemented /health endpoint",
    next_steps=["Add auth middleware"],
    files=["apps/api/main.py"],
)

# Claude resumes in same repo
claude = connect_project(agent="claude")
for h in claude.recall_handoffs():
    print(h.format_brief())
```

## MCP + hooks

Re-run `fluctlight-project init` from a real repo root to materialize `.cursor/` and `.claude/` templates. Hooks require Cursor; MCP works in any client that supports stdio MCP servers.
