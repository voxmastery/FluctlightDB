# FluctlightDB onboarding (5 minutes)

**Multi-agent project memory for Cursor, Claude Code, and Codex** — one repo, shared handoffs, no hosted SaaS required.

## 1. Install

```bash
pip install "fluctlightdb[native,mcp]"
```

Or run the wizard:

```bash
fluctlight-project onboard
```

## 2. Initialize your repo

From monorepo root:

```bash
fluctlight-project init --team-sync
fluctlight-project doctor
```

This creates:

- Shared project brain + handoff inbox
- Cursor MCP + hooks + **required rules** (memory is default-on)
- Claude MCP settings + skill
- Codex MCP template

## 3. View handoffs

```bash
fluctlight-project ui
```

Open **http://127.0.0.1:8787** — handoff inbox in your browser.

## 4. Sync across machines (VPS + laptop)

```bash
fluctlight-project sync pull    # before session
fluctlight-project sync push    # after session
```

Full guide: [VPS_DESKTOP.md](VPS_DESKTOP.md)

## 5. Use in code

```python
from fluctlightdb import connect_agent

brain = connect_agent()
brain.turn_begin()
brain.wm_push("Use pytest for tests", context="project", salience=0.7)
print(brain.recall("test framework"))
brain.turn_end(flush=True)
```

Multi-agent project brains still use `connect_project()` for handoffs — see [MULTI_AGENT.md](MULTI_AGENT.md).

## vs Mem0 / Zep

| | Mem0 / Zep | FluctlightDB |
|--|------------|--------------|
| Model | Hosted API + chat extraction | Embedded **database engine** |
| Multi-tool monorepo | Bring your own glue | `fluctlight-project init` |
| Data location | Their cloud (typical) | **Your disk / your VPS / your git** |
| Benchmarks | Their numbers | Public LoCoMo 98.1% evidence recall |

FluctlightDB is **not** a hosted memory SaaS — it's SQLite-for-agents at the repo level. You own the files.

## Next

- [MULTI_AGENT.md](MULTI_AGENT.md) — full API
- [PLATFORM_COMPAT.md](PLATFORM_COMPAT.md) — Windows / macOS / Linux
- [README](../README.md) — benchmarks and engine overview
