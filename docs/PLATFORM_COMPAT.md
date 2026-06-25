# Platform compatibility (multi-agent project brains)

FluctlightDB project brains (`fluctlight-project init`, `connect_project()`) are supported on **Linux, macOS, and Windows**.

## OS matrix

| Feature | Linux | macOS | Windows |
|---------|-------|-------|---------|
| `fluctlight-project init` | Yes | Yes | Yes |
| Embedded native brain | Yes (wheels) | Yes (arm64/x86_64 wheels) | Yes (x64 wheels) |
| Cross-process locks | `filelock` + Rust `fs2` | Same | Same |
| Cursor MCP + hooks | Yes | Yes | Yes (`.cmd` wrappers + `py -3`) |
| Claude MCP (settings.json) | Yes | Yes | Yes |
| Codex MCP template | Yes | Yes | Yes |
| Handoff inbox (`.fluctlight/handoffs.jsonl`) | Yes | Yes | Yes |

## Python command resolution

`fluctlight-project init` renders MCP configs with a platform-appropriate Python:

| OS | MCP `command` | Typical `args` |
|----|---------------|----------------|
| Windows | `py` (if installed) or `sys.executable` | `["-3", "-m", "fluctlightdb.mcp_server"]` |
| Linux / macOS | `python3` or `sys.executable` | `["-m", "fluctlightdb.mcp_server"]` |

Run `fluctlight-project doctor` to see the resolved command on your machine.

## Lock files

| Path | Purpose |
|------|---------|
| `.fluctlight/project/.brain.lock` | Rust engine + Python SDK serialize brain writes |
| `.fluctlight/.handoffs.lock` | Handoff JSONL index append lock |

Both use the same cross-platform locking stack (`filelock` in Python, `fs2` in Rust).

## Windows notes

- Hooks in `.cursor/hooks.json` point to `.cmd` wrappers that invoke `py -3 script.py`.
- Install Python from python.org or Microsoft Store; the `py` launcher is recommended.
- If MCP fails to start, run `fluctlight-project doctor` and fix the reported Python path.

## macOS notes

- Apple Silicon and Intel wheels are published on PyPI as `fluctlightdb-native`.
- File locking uses the same code path as Linux.

## Serve vs embedded

Do not point `fluctlight-serve` and embedded `connect_project()` at the **same brain directory** simultaneously. The engine enforces exclusive locks for up to 120s.

Set only one access mode per brain path:

- **Embedded** (default for project brains): `connect_project()` / MCP / hooks
- **HTTP serve**: `FLUCTLIGHT_SERVE_URL` + `FluctlightClient` — use a separate brain path

## Team sync (optional)

```bash
fluctlight-project init --team-sync
```

- Commits `.fluctlight/project/` and `handoffs.jsonl` to git
- Keeps `.fluctlight/agents/` local (per developer)

See `.fluctlight/TEAM_SYNC.md` after init.

## Verify your setup

```bash
pip install "fluctlightdb[native,mcp]"
fluctlight-project init
fluctlight-project doctor
fluctlight-project handoffs --json
```
