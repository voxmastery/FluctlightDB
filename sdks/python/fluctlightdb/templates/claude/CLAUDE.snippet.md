## FluctlightDB (project memory)

This monorepo uses FluctlightDB (`{{project_id}}`) for durable agent memory:

- Hub: `.fluctlight/project/` (shared decisions + handoffs)
- Spokes: `.fluctlight/agents/{cursor,claude,codex}/`

Use `from fluctlightdb import connect_project` and call `session_context()`, `recall()`, `remember()`, and `handoff()` when switching agents or resuming work. See `.claude/skills/fluctlight-memory/SKILL.md`.
