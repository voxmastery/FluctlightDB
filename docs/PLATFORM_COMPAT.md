# Platform compatibility (multi-agent project brains)

FluctlightDB project brains (`fluctlight-project init`, `connect_project()`) are supported on **Linux, macOS, and Windows**.

## OS matrix

| Feature | Linux | macOS | Windows |
|---------|-------|-------|---------|
| `fluctlight-project init` | Yes | Yes | Yes |
| Embedded native brain (`fluctlightdb-native`) | Yes — abi3 manylinux x86_64 | Yes — abi3 universal2 (Intel + Apple Silicon) | Yes — abi3 win_amd64 |
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

## Native wheels (PyPI `fluctlightdb-native`)

| Platform | Wheel tag | Python |
|----------|-----------|--------|
| Linux x86_64 | `manylinux_2_17_x86_64` | 3.9–3.13 (abi3) |
| macOS | `macosx_*_universal2` | 3.9–3.13 (abi3) |
| Windows x64 | `win_amd64` | 3.9–3.13 (abi3) |

```bash
pip install "fluctlightdb[native]>=0.5.4"
```

**Linux arm64** and **Windows arm64**: no prebuilt wheel yet — use **sdist** (`pip install fluctlightdb-native --no-binary :all:`) with Rust installed, or HTTP-only `pip install fluctlightdb` (no native extension).

## macOS notes

- Universal2 wheel covers Apple Silicon and Intel Macs.
- File locking uses the same code path as Linux.

## Windows notes

- Hooks in `.cursor/hooks.json` point to `.cmd` wrappers that invoke `py -3 script.py`.
- Install Python from python.org or Microsoft Store; the `py` launcher is recommended.
- If MCP fails to start, run `fluctlight-project doctor` and fix the reported Python path.

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
