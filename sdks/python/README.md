# fluctlightdb

Python client for [FluctlightDB](https://github.com/voxmastery/FluctlightDB) — a brain-native memory store for AI agents.

## Install

```bash
pip install fluctlightdb
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
