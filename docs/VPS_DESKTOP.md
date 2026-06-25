# VPS Cursor CLI + local desktop — shared project brain

**Yes — you can use FluctlightDB with Cursor CLI on a VPS and Cursor on your local desktop**, sharing the same project memory. Two patterns:

## Pattern A: Git team-sync (simplest, recommended)

Both machines clone the **same repo**. Shared files live in git:

- `.fluctlight/project/` — shared hub brain
- `.fluctlight/handoffs.jsonl` — handoff inbox
- `.fluctlight/config.yaml`

Per-machine (not synced):

- `.fluctlight/agents/cursor/` — local session notes

### Setup (once)

```bash
fluctlight-project init --team-sync
git add .fluctlight .cursor .claude .gitignore
git commit -m "Add FluctlightDB project brain"
git push
```

### VPS (Cursor CLI / agent)

```bash
git pull
fluctlight-project sync pull
# ... work with Cursor CLI ...
fluctlight-project sync push -m "fluctlight: VPS session handoff"
```

### Local desktop (Cursor IDE)

```bash
git pull
fluctlight-project sync pull
fluctlight-project ui   # optional: view handoff inbox at http://127.0.0.1:8787
# ... work in Cursor ...
fluctlight-project sync push
```

Handoffs and conventions written on the VPS appear on your desktop after `sync pull`, and vice versa.

---

## Pattern B: Live hub on VPS (HTTP, no git latency)

Run **fluctlight-serve** on the VPS with the project brain. Local desktop connects over HTTP.

### VPS

```bash
# Point serve at your repo's project brain (example)
export FLUCTLIGHT_BRAIN_PATH=/path/to/repo/.fluctlight/project
docker run -d -p 8792:8792 \
  -e FLUCTLIGHT_API_KEYS=default:YOUR_SECRET:write \
  -v /path/to/repo/.fluctlight/project:/data/brain \
  ghcr.io/voxmastery/fluctlightdb:latest
```

Use TLS (Caddy/nginx) for any public bind — see [DEPLOYMENT.md](DEPLOYMENT.md).

### Local desktop + VPS CLI

In repo `.fluctlight/config.yaml`:

```yaml
serve:
  enabled: true
  url: https://fluctlight.your-vps.example.com
```

Or environment:

```bash
export FLUCTLIGHT_HUB_URL=https://fluctlight.your-vps.example.com
export FLUCTLIGHT_API_KEY=YOUR_SECRET
```

MCP / hooks / Python:

```python
from fluctlightdb import connect_project
pb = connect_project()  # auto-uses remote hub when FLUCTLIGHT_HUB_URL or serve.enabled
pb.handoff("Done on VPS", files=["src/main.py"])
```

Local agent spokes (`.fluctlight/agents/cursor/`) stay embedded on each machine; **shared hub** is on the VPS.

---

## Which pattern?

| | Git sync | HTTP hub |
|--|----------|----------|
| Setup | Easy | Needs serve + TLS |
| Latency | Pull/push cadence | Live |
| Offline | Yes (local brain) | Needs network |
| Best for | Solo/small team, same repo | Always-on VPS agent |

---

## Cursor CLI on VPS notes

- Install: `pip install "fluctlightdb[native,mcp]"`
- Run `fluctlight-project init --team-sync` in repo root
- MCP config is in `.cursor/mcp.json` (same as desktop)
- Use `fluctlight-project doctor` on both machines after setup

## Troubleshooting

| Issue | Fix |
|-------|-----|
| Handoffs missing on other machine | `fluctlight-project sync pull` or check hub URL |
| Lock busy | Don't run embedded + serve on same brain path |
| MCP won't start on Windows VPS | `fluctlight-project doctor` — check `py -3` path |

See also: [MULTI_AGENT.md](MULTI_AGENT.md) · [PLATFORM_COMPAT.md](PLATFORM_COMPAT.md)
