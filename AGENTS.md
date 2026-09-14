## FluctlightDB (project memory)

This monorepo uses FluctlightDB (`FluctlightDB`) for durable agent memory:

- Hub: `.fluctlight/project/` (shared decisions + handoffs)
- Spokes: `.fluctlight/agents/{cursor,claude,codex}/`

Use `from fluctlightdb import connect_project` and call `session_context()`, `recall()`, `remember()`, and `handoff()` when switching agents or resuming work. See `.claude/skills/fluctlight-memory/SKILL.md`.

**Serve next to an agent:** pin one `FLUCTLIGHT_BIN`, run `scripts/preflight-serve.sh`, and read
`docs/runbooks/hermes-style-agent-upgrade.md` before bumping 0.5.x. Hermes-style rule: memory is a
sidecar (HTTP). Do not glob-newest and do not let mindloop/CLI take the live store lock.
