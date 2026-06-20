# FluctlightDB

**A brain-native database for AI agents.** Not a vector database. Not SQL.

Give your agent a **mind it can grow** — episodic memory, verified facts, and recall by activation (not just cosine similarity).

**Repo:** [github.com/voxmastery/FluctlightDB](https://github.com/voxmastery/FluctlightDB)

---

## Install (Python agents — recommended)

Like **`sqlite3`** (library in your process) or **`pip install qdrant-client`** (SDK + optional server):

```bash
git clone https://github.com/voxmastery/FluctlightDB.git
cd FluctlightDB

# One-time: build the native extension (~2 min)
./scripts/install-native.sh

# Use the Python SDK (no cargo in your agent code)
pip install -e sdks/python   # optional; or add sdks/python to PYTHONPATH
```

```python
import os
os.environ["FLUCTLIGHT_NATIVE"] = "1"

from fluctlightdb import get_recall_client

brain = get_recall_client("~/.fluctlight/tenants/default/brain")

# Recall — sub-ms, in-process (like sqlite3.execute)
print(brain.activate("what did the user prefer for theme"))

# Writes go through HTTP serve (see below) or Rust CLI
```

That’s the **recommended industrial path**: native library for hot recall, HTTP for writes when needed.

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
| **README leads with** | `sqlite3`, any language | `pip install`, Docker, REST | **`pip` + native lib** (this README) |
| **First line of code** | `import sqlite3` | `from qdrant_client import …` | `from fluctlightdb import get_recall_client` |
| **Query** | SQL strings | vector + filter | **`activate(cue)`** |
| **Explore data** | `sqlite3`, DBeaver | Web UI / scroll API | **`fluctlight shell`** |
| **Build from source** | optional (amalgamation) | optional (Rust) | optional (`cargo build` — for contributors) |

SQL and vector READMEs **don’t ask app devs to run `cargo`**. Neither should we — `cargo` is only for building Fluctlight itself or the native wheel once.

---

## Quick start (5 minutes)

### 1. Interactive REPL (like `psql` or `sqlite3`)

```bash
# Requires one-time: cargo build --release  (or download a release binary later)
./target/release/fluctlight shell --local --path /tmp/my-agent-brain
```

```
fluctlight> experience user prefers dark mode
fluctlight> recall dark mode
fluctlight> list 5
fluctlight> quit
```

### 2. Python agent (recommended)

```bash
./scripts/install-native.sh
export FLUCTLIGHT_NATIVE=1
python3 -c "
from fluctlightdb import get_recall_client
print(get_recall_client('/tmp/my-agent-brain').activate('dark mode'))
"
```

### 3. HTTP server (writes, multi-tenant — like Qdrant Docker)

```bash
./target/release/fluctlight tenant provision myagent --role admin
./target/release/fluctlight serve --path ~/.fluctlight/tenants/myagent/brain
```

```python
from fluctlightdb import FluctlightClient
client = FluctlightClient.from_env()
client.experience("deployment succeeded", context="ci")
client.activate_lite("last deployment")  # HTTP keep-alive, top-1 only
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

## Build from source (contributors / no wheel yet)

```bash
cargo build --release
./target/release/fluctlight --help
```

Rust API:

```rust
use fluctlightdb::{Episode, FluctlightBrain};

let mut brain = FluctlightBrain::open("/path/to/brain").unwrap();
brain.experience(Episode::new("fixed cache bug", "debug", 0.8)).unwrap();
let recalls = brain.activate("cache bug");
```

---

## Docs

- **[Getting started](docs/GETTING_STARTED.md)** — UX deep-dive, FAQ  
- [CLI.md](docs/CLI.md) — SQL/vector → Fluctlight commands  
- [DEPLOYMENT.md](docs/DEPLOYMENT.md) — replica, backup, industrial HA  
- [Manifesto.md](docs/Manifesto.md) — philosophy  
- [openapi.yaml](docs/openapi.yaml) — HTTP API  

## Contributing

- [CONTRIBUTING.md](CONTRIBUTING.md) — build, test, PR guidelines  
- [SECURITY.md](SECURITY.md) — report vulnerabilities privately  
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)  

## License

MIT OR Apache-2.0 — see `LICENSE` (GitHub), `LICENSE-MIT`, `LICENSE-APACHE`.
