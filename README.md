# FluctlightDB

**Long-term memory for AI agents** — store what your agent experienced, recall it by meaning, and tell verified facts from chat guesses.

[![PyPI](https://img.shields.io/pypi/v/fluctlightdb)](https://pypi.org/project/fluctlightdb/) · [GitHub](https://github.com/voxmastery/FluctlightDB)

> **Not** a vector database. **Not** SQL. Episodic memory: things your agent *lived through*, with provenance.

---

## What is this?

If you build agents (coding assistants, workers, NPCs, companions), they need memory beyond the current chat window.

| Approach | Good at | Weak at |
|----------|---------|---------|
| **SQL** (Postgres, SQLite) | Structured rows, reports | “What did we decide last week?” |
| **Vector DB** (Qdrant, Pinecone) | Similarity search | Truth — chat claim vs ledger fact |
| **FluctlightDB** | **Episodes + recall + provenance** | Replacing your billing database |

**Mental model:** one **brain folder per agent** on disk — like one SQLite file or one Qdrant collection path. Your code **experiences** events and **activates** recall from a cue (not SQL strings, not pure cosine similarity).

| Goal | API |
|------|-----|
| Remember something that happened | `experience(...)` |
| Recall by cue / meaning | `activate(cue)` |
| Know what's verified vs guessed | provenance on each engram |
| Persist | `checkpoint()` (embedded) or server writes to disk |

---

## Quick start (~2 minutes)

**Recommended:** in-process Python, like `sqlite3` — no server, no Docker, no Rust.

On Debian/Ubuntu/Fedora, use a venv ([PEP 668](https://peps.python.org/pep-0668/) blocks global `pip`):

```bash
python3 -m venv .venv
source .venv/bin/activate   # Windows: .venv\Scripts\activate
pip install "fluctlightdb[native]"
```

```python
from fluctlightdb import connect

brain = connect("/tmp/my-agent-brain")   # creates a folder on disk
brain.experience("User prefers dark mode", context="settings", salience=0.8)
print(brain.activate("theme preference"))
brain.checkpoint()
```

That's it. Open the folder, write experiences, recall by cue.

---

## Choose your path

```
Solo agent, lowest latency (most people start here)
  pip install "fluctlightdb[native]"
  from fluctlightdb import connect
  brain = connect("/path/to/brain")

Several agents / remote server / ops runs the DB
  pip install fluctlightdb
  Docker or release binary → FluctlightClient over HTTP

Terminal exploration
  fluctlight shell  (needs server binary from Releases)

Change the Rust core or CLI
  clone + cargo — see CONTRIBUTING.md
```

### Optional: HTTP client + server

When you need a shared or remote brain (like `qdrant-client` + Qdrant server):

```bash
docker pull ghcr.io/voxmastery/fluctlightdb:latest
docker run -p 8792:8792 \
  -e FLUCTLIGHT_API_KEYS=default:your-secret:write \
  -v fluctlight-data:/data \
  ghcr.io/voxmastery/fluctlightdb:latest
```

```python
import os

os.environ["FLUCTLIGHT_SERVE_URL"] = "http://127.0.0.1:8792"
os.environ["FLUCTLIGHT_API_KEY"] = "your-secret"

from fluctlightdb import FluctlightClient

client = FluctlightClient.from_env()
client.experience("Deploy succeeded", context="ci")
print(client.activate("last deployment"))
```

Release binaries (no `cargo`): [GitHub Releases](https://github.com/voxmastery/FluctlightDB/releases). Production layout: [DEPLOYMENT.md](docs/DEPLOYMENT.md) · [DOCKER.md](docs/DOCKER.md).

---

## How is this different?

| | Vector DB | SQL | FluctlightDB |
|---|-----------|-----|--------------|
| **Unit of storage** | point + embedding | row | **engram** (episode) |
| **Query** | nearest neighbor | `SELECT … WHERE` | **`activate(cue)`** |
| **Truth** | payload flags | your columns | **ledger beats chat** |
| **Typical install** | pip client + server | driver + server | **`pip install fluctlightdb`** |

Deeper comparisons (REPL, CLI mapping, FAQ): **[Getting started](docs/GETTING_STARTED.md)**

---

## Performance (measured)

Linux x86_64, in-process [`prod_bench`](crates/fluctlightdb/tests/prod_bench.rs):

| Path | Latency |
|------|---------|
| **Embedded** (`connect`) | ~**0.002 ms** / `activate` |
| **HTTP** (`FluctlightClient`, keep-alive) | ~1–5 ms / request on localhost |

On `"wallet balance"`, verified ledger (`$0.00`) ranks above unverified chat claims. Reproduce: `./scripts/manifesto-audit.sh`.

---

## Documentation

| Doc | Who it's for |
|-----|----------------|
| **[Getting started](docs/GETTING_STARTED.md)** | First visit — paths, storage, FAQ |
| [CLI.md](docs/CLI.md) | `fluctlight shell`, SQL/vector habit mapping |
| [DOCKER.md](docs/DOCKER.md) | Container image |
| [DEPLOYMENT.md](docs/DEPLOYMENT.md) | Backup, replica, multi-tenant |
| [Manifesto.md](docs/Manifesto.md) | Why brain-native memory |
| [openapi.yaml](docs/openapi.yaml) | HTTP API contract |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Rust / core development |

---

## Contributing

**Using Fluctlight in an agent?** `pip install fluctlightdb` — no Rust required.

**Changing the database, CLI, or server?** See [CONTRIBUTING.md](CONTRIBUTING.md). Report security issues via [SECURITY.md](SECURITY.md).

## License

MIT OR Apache-2.0 — see `LICENSE`, `LICENSE-MIT`, `LICENSE-APACHE`.
