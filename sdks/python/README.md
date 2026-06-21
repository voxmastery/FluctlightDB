# fluctlightdb

Python client for [FluctlightDB](https://github.com/voxmastery/FluctlightDB) — a brain-native memory store for AI agents.

## Install

On **Debian/Ubuntu 23.04+**, **Debian 12+**, and **Fedora 38+**, system Python is [PEP 668](https://peps.python.org/pep-0668/) *externally managed* — bare `pip install` fails with `externally-managed-environment`. Use a **virtual environment** (same as any other PyPI library):

```bash
python3 -m venv .venv
source .venv/bin/activate   # Windows: .venv\Scripts\activate
pip install fluctlightdb
```

From a clone of this repo you can also run:

```bash
./scripts/install-python-client.sh
```

Optional in-process recall (Rust extension, when wheels are available for your platform):

```bash
pip install "fluctlightdb[native]"
# or: pip install fluctlightdb-native
```

No `cargo` or Rust toolchain required for the HTTP client.

## Quick start (HTTP — like `qdrant-client`)

Run a FluctlightDB server (download a [release binary](https://github.com/voxmastery/FluctlightDB/releases) or use your own deployment), then:

```python
from fluctlightdb import FluctlightClient

client = FluctlightClient(
    base_url="http://127.0.0.1:8792",
    api_key="your-key",
)

client.experience("user prefers dark mode", context="settings")
print(client.activate_lite("theme preference"))
```

Or use environment variables:

```bash
export FLUCTLIGHT_SERVE_URL=http://127.0.0.1:8792
export FLUCTLIGHT_API_KEY=your-key
```

```python
from fluctlightdb import FluctlightClient

client = FluctlightClient.from_env()
print(client.activate("dark mode"))
```

## In-process recall (optional)

When `fluctlightdb-native` is installed:

```python
from fluctlightdb import get_recall_client

brain = get_recall_client("~/.fluctlight/tenants/default/brain")
print(brain.activate("dark mode"))
```

## Docs

- [Getting started](https://github.com/voxmastery/FluctlightDB/blob/main/docs/GETTING_STARTED.md)
- [HTTP API (OpenAPI)](https://github.com/voxmastery/FluctlightDB/blob/main/docs/openapi.yaml)

## License

MIT — see [LICENSE](LICENSE).
