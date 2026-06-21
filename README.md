# FluctlightDB

**A brain-native database for AI agents.** Not a vector database. Not SQL.

Give your agent a **mind it can grow** — episodic memory, verified facts, and recall by activation (not just cosine similarity).

[![PyPI](https://img.shields.io/pypi/v/fluctlightdb)](https://pypi.org/project/fluctlightdb/)
**Repo:** [github.com/voxmastery/FluctlightDB](https://github.com/voxmastery/FluctlightDB)

---

## Install (Python agents — recommended)

Like **`pip install qdrant-client`** — no Rust toolchain required:

```bash
pip install fluctlightdb
```

Optional in-process recall (prebuilt wheels when available for your OS):

```bash
pip install "fluctlightdb[native]"
```

```python
from fluctlightdb import FluctlightClient

client = FluctlightClient.from_env()  # FLUCTLIGHT_SERVE_URL + FLUCTLIGHT_API_KEY
client.experience("user prefers dark mode", context="settings")
print(client.activate_lite("theme preference"))
```

Point at a running FluctlightDB server — download a [release binary](https://github.com/voxmastery/FluctlightDB/releases) or use your own deployment (see [DEPLOYMENT.md](docs/DEPLOYMENT.md)). **Agent code never needs `cargo`.**

### Optional: in-process recall

```python
from fluctlightdb import get_recall_client

brain = get_recall_client("~/.fluctlight/tenants/default/brain")
print(brain.activate("dark mode"))  # sub-ms when fluctlightdb-native is installed
```

---

## How is storage “one thing”?

| System | What you point at | Feels like |
|--------|-------------------|------------|
| **SQLite** | One file: `agent.db` | Single portable file |
| **Qdrant (local)** | One folder: `./qdrant_storage/` | Directory + segments |
| **FluctlightDB (v4)** | One folder: `~/.fluctlight/tenants/default/brain/` | Directory + `recall_index.sqlite` sidecar |

So yes — **one brain per agent** is the right mental model (like one SQLite file or one Qdrant collection path). Under the hood v4 is a **small directory**, not a single `.flct` blob — same idea as Qdrant local or Postgres data dir.

Legacy single-file `.flct` still loads; new installs use the v4 directory layout.

---

## Developer UX vs SQL vs Vector

| | **SQLite** | **Qdrant / Pinecone** | **FluctlightDB** |
|---|------------|----------------------|------------------|
| **README leads with** | `sqlite3`, any language | `pip install`, Docker, REST | **`pip install fluctlightdb`** |
| **First line of code** | `import sqlite3` | `from qdrant_client import …` | `from fluctlightdb import FluctlightClient` |
| **Query** | SQL strings | vector + filter | **`activate(cue)`** |
| **Explore data** | `sqlite3`, DBeaver | Web UI / scroll API | **`fluctlight shell`** (server binary) |
| **Build from source** | optional | optional | **optional** — Rust only for [contributors](#contributing) |

---

## Quick start (5 minutes)

### 1. Install the Python SDK

```bash
pip install fluctlightdb
```

### 2. Run a server (operators)

Download `fluctlight` from [GitHub Releases](https://github.com/voxmastery/FluctlightDB/releases) (or build from source if you maintain the server):

```bash
fluctlight tenant provision myagent --role admin
fluctlight serve --path ~/.fluctlight/tenants/myagent/brain
```

### 3. Use from your agent

```python
import os
os.environ["FLUCTLIGHT_SERVE_URL"] = "http://127.0.0.1:8792"
os.environ["FLUCTLIGHT_API_KEY"] = "your-key"

from fluctlightdb import FluctlightClient
client = FluctlightClient.from_env()
client.experience("deployment succeeded", context="ci")
print(client.activate_lite("last deployment"))
```

### 4. Interactive REPL (optional — needs server binary)

```bash
fluctlight shell --local --path /tmp/my-agent-brain
```

```
fluctlight> experience user prefers dark mode
fluctlight> recall dark mode
fluctlight> quit
```

---

## Operator UX (terminal)

| You want to… | SQLite | Vector DB | Fluctlight |
|--------------|--------|-----------|------------|
| Connect | `sqlite3 db` | Dashboard / curl | `fluctlight shell` |
| Browse | `SELECT * LIMIT 10` | scroll | `list 10` |
| Search | `WHERE … LIKE` | similarity search | `recall <cue>` |
| Truth | your columns | payload | `verified` / `warnings` |

Full mapping: [docs/CLI.md](docs/CLI.md)

---

## Why not just SQL or vectors?

| Today | Problem | Fluctlight |
|-------|---------|------------|
| Vector DB | Similarity ≠ lived memory | Engrams + spreading activation |
| SQL | Rows without provenance/growth | Provenance, sleep, maturation |
| RAG | Memory outside the agent | Experience inside the agent |

---

## Build from source (contributors)

**Using Fluctlight in your agent?** Use `pip install fluctlightdb` — no Rust required.

**Changing the database, CLI, or server?** Clone and install [Rust (stable)](https://rustup.rs), then:

```bash
git clone https://github.com/voxmastery/FluctlightDB.git
cd FluctlightDB
cargo build --release
cargo test --release
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for repo layout, dev commands, and what to work on without vs with Rust.

---

## Docs

- **[Getting started](docs/GETTING_STARTED.md)** — UX deep-dive, FAQ  
- [CLI.md](docs/CLI.md) — SQL/vector → Fluctlight commands  
- [DEPLOYMENT.md](docs/DEPLOYMENT.md) — replica, backup, industrial HA  
- [PUBLISHING.md](docs/PUBLISHING.md) — PyPI releases (maintainers)  
- [Manifesto.md](docs/Manifesto.md) — philosophy  
- [openapi.yaml](docs/openapi.yaml) — HTTP API  

## Contributing

FluctlightDB is a **Rust codebase** with a **Python client on PyPI**. Agent users only need `pip`; core contributors need [Rust + the guide below](CONTRIBUTING.md).

- [CONTRIBUTING.md](CONTRIBUTING.md) — Rust setup, repo layout, where to start  
- [SECURITY.md](SECURITY.md) — report vulnerabilities privately  
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)  

## License

MIT OR Apache-2.0 — see `LICENSE` (GitHub), `LICENSE-MIT`, `LICENSE-APACHE`.
