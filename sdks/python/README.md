# fluctlightdb

Python client for [FluctlightDB](https://github.com/voxmastery/FluctlightDB) — episodic memory for AI agents (not a vector DB, not SQL).

## Install

On **Debian/Ubuntu 23.04+**, **Debian 12+**, and **Fedora 38+**, use a **virtual environment** ([PEP 668](https://peps.python.org/pep-0668/)):

```bash
python3 -m venv .venv
source .venv/bin/activate   # Windows: .venv\Scripts\activate
pip install "fluctlightdb[native]"
```

From a clone: `./scripts/install-python-client.sh` (HTTP client only; add `[native]` for embedded).

HTTP-only client (no Rust extension):

```bash
pip install fluctlightdb
```

## Quick start — embedded (recommended)

Like `sqlite3` — no server:

```python
from fluctlightdb import connect

brain = connect("/tmp/my-agent-brain")
brain.experience("User prefers dark mode", context="settings")
print(brain.activate("dark mode"))
brain.checkpoint()
```

Read-only recall: `get_recall_client(path)`.

## Quick start — HTTP (shared / remote server)

Run a [release binary](https://github.com/voxmastery/FluctlightDB/releases) or Docker image, then:

```python
from fluctlightdb import FluctlightClient

client = FluctlightClient.from_env()  # FLUCTLIGHT_SERVE_URL + FLUCTLIGHT_API_KEY
client.experience("User prefers dark mode", context="settings")
print(client.activate("dark mode"))
```

## Docs

- [Getting started](https://github.com/voxmastery/FluctlightDB/blob/main/docs/GETTING_STARTED.md)
- [HTTP API (OpenAPI)](https://github.com/voxmastery/FluctlightDB/blob/main/docs/openapi.yaml)
