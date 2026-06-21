# FluctlightDB

**A brain-native database for AI agents.** Not a vector database. Not SQL.

Give your agent a **mind it can grow** — episodic memory, verified facts, and recall by activation (not just cosine similarity).

[![PyPI](https://img.shields.io/pypi/v/fluctlightdb)](https://pypi.org/project/fluctlightdb/)
**Repo:** [github.com/voxmastery/FluctlightDB](https://github.com/voxmastery/FluctlightDB)

---

## Install (Python agents — recommended)

Use a **venv** on Debian/Ubuntu/Fedora (PEP 668 blocks global pip):

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install "fluctlightdb[native]==0.4.3"
```

### Embedded brain — direct library call (like `sqlite3`)

No server, no HTTP, no `fluctlight` binary. The Rust core runs **in your Python process**:

```python
from fluctlightdb import connect

brain = connect("/tmp/my-agent-brain")          # open brain folder on disk
brain.experience("user prefers dark mode", context="settings", salience=0.8)
print(brain.activate("dark mode"))              # direct Rust call, not network
brain.checkpoint()
```

This is the same pattern as `import sqlite3` — **library call, not client/server**.

### HTTP client — when you need a remote or shared server

Like `psycopg2` → Postgres. Install the pure-Python client and point at Docker or a [release binary](https://github.com/voxmastery/FluctlightDB/releases):

```bash
pip install fluctlightdb
```

```python
from fluctlightdb import FluctlightClient

client = FluctlightClient.from_env()  # FLUCTLIGHT_SERVE_URL + FLUCTLIGHT_API_KEY
client.experience("user prefers dark mode", context="settings")
print(client.activate_lite("theme preference"))
```

```bash
docker pull ghcr.io/voxmastery/fluctlightdb:latest
docker run -p 8792:8792 \
  -e FLUCTLIGHT_API_KEYS=default:your-secret-key:write \
  -v fluctlight-data:/data \
  ghcr.io/voxmastery/fluctlightdb:latest
```

**Agent code never needs `cargo`.** See [DEPLOYMENT.md](docs/DEPLOYMENT.md) for ops/HA.

---

## Performance (measured)

Numbers below are from this repo’s release benchmarks on Linux x86_64 — re-run anytime to verify on your machine.

| Path | Typical hot-path latency | What it is |
|------|--------------------------|------------|
| **Embedded native** (`connect`) | **~0.002 ms / `activate`** | Direct Rust library call in-process ([`prod_bench`](crates/fluctlightdb/tests/prod_bench.rs)) |
| **HTTP** (`FluctlightClient`, keep-alive) | ~1–5 ms / request | Client/server over localhost |
| **Vector DB ANN** (typical) | ~2–5 ms / query | Industry ballpark for remote similarity search |

**Recall quality** (same `prod_bench` fixture, 1000 queries):

| Method | Hits / 1000 |
|--------|-------------|
| Fluctlight activation | **1000 / 1000** |
| Lexical substring scan | 800 / 1000 |
| Brute cosine vector (≥ 0.75) | 1000 / 1000 |

**Truth / provenance:** on cue `wallet balance`, verified ledger (`$0.00`) ranks above unverified chat claims — checked in `prod_bench` and [`manifesto_audit`](crates/fluctlightdb/tests/manifesto_audit.rs).

**Server observability** (Docker / `fluctlight serve`): Prometheus metrics at `GET /metrics` — `fluctlight_activate_ms_avg`, `fluctlight_experience_ms_avg`, `fluctlight_synapse_count`, per-tenant counters.

Reproduce locally:

```bash
cargo test --release -p fluctlightdb --test prod_bench -- --nocapture
./scripts/manifesto-audit.sh
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
| **Agent hot path** | `import sqlite3` (in-process) | HTTP/gRPC client | **`connect()` native** (in-process) or HTTP client |
| **README leads with** | `sqlite3`, any language | `pip install`, Docker, REST | **`pip install fluctlightdb[native]`** + optional Docker |
| **First line of code** | `import sqlite3` | `from qdrant_client import …` | `from fluctlightdb import connect` |
| **Query** | SQL strings | vector + filter | **`activate(cue)`** |
| **Explore data** | `sqlite3`, DBeaver | Web UI / scroll API | **`fluctlight shell`** (server binary) |
| **Build from source** | optional | optional | **optional** — Rust only for [contributors](#contributing) |

---

## Quick start (5 minutes)

### 1. Install the Python SDK (embedded — recommended)

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install "fluctlightdb[native]==0.4.3"
```

```python
from fluctlightdb import connect

brain = connect("/tmp/my-agent-brain")
brain.experience("user prefers dark mode", context="settings")
print(brain.activate("dark mode"))
```

### 2. Run a server (optional — multi-agent / remote)

**Docker (recommended):**

```bash
docker pull ghcr.io/voxmastery/fluctlightdb:latest
docker run -d -p 8792:8792 \
  -e FLUCTLIGHT_API_KEYS=default:your-secret-key:write \
  -v fluctlight-data:/data \
  ghcr.io/voxmastery/fluctlightdb:latest
```

Or download `fluctlight` from [GitHub Releases](https://github.com/voxmastery/FluctlightDB/releases) (Linux / macOS tarballs).

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
- [DOCKER.md](docs/DOCKER.md) — Docker image and compose  
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
