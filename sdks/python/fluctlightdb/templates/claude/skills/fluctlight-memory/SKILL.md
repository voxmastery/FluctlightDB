---
name: fluctlight-memory
description: >-
  Use FluctlightDB project brains for durable memory, recall, and handoffs between
  Cursor, Claude Code, and Codex in this monorepo. Call when you need prior decisions,
  conventions, cross-agent context, or to leave a handoff for another tool.
---

# FluctlightDB project memory

This repo uses **FluctlightDB** as the shared project brain (`{{project_id}}`).

## Layout

- `.fluctlight/project/` — shared hub (decisions, handoffs, conventions)
- `.fluctlight/agents/<tool>/` — per-agent spoke (session-local notes)

## When to use

1. **Before big changes** — recall conventions and prior decisions for the current subdirectory.
2. **After completing work** — write a handoff if another agent may continue.
3. **When unsure** — recall by cue instead of re-deriving from chat history.

## Python (embedded)

```python
from fluctlightdb import connect_project

pb = connect_project(agent="claude")  # or cursor / codex
print(pb.session_context())

pb.remember("API uses JWT in Authorization header", scope="project", context="architecture")
hits = pb.recall("authentication conventions", scope="all")
pb.handoff("Finished auth middleware", next_steps=["Add integration tests"], files=["src/auth.py"])
```

## MCP (if configured)

Use tools: `fluctlight_recall`, `fluctlight_remember`, `fluctlight_handoff`, `fluctlight_session_context`, `fluctlight_status`.

## Handoff protocol

Handoffs are JSON episodes with context `handoff:<agent>:<subdir>`. Other agents see them via `recall_handoffs()` / `session_context()`.

## Rules

- Prefer **project** scope for team-wide facts; **agent** scope for tool-specific session notes.
- Do not store secrets or credentials in brains.
- Keep handoff summaries short and actionable (status, next steps, touched files).
