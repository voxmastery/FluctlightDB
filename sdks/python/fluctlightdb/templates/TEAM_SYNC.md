# Team sync mode (FluctlightDB)

This repo uses **team-sync** for the shared project brain:

- **Commit** `.fluctlight/project/` — shared decisions, conventions, handoffs
- **Do not commit** `.fluctlight/agents/` — per-developer session notes stay local
- Handoff inbox: `.fluctlight/handoffs.jsonl` (commit for cross-agent visibility)

Resolve conflicts in `handoffs.jsonl` like a log: keep both lines, newest wins at read time.

Run `fluctlight-project doctor` after pull to verify native + MCP setup.
