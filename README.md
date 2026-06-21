# FluctlightDB

**A database engine built for AI agents.**

Give each agent its own persistent store: **write** what happened, **recall** it later from a short cue, and **prefer trusted sources** (logs, ledgers, files) over things the model said in chat.

[![PyPI](https://img.shields.io/pypi/v/fluctlightdb)](https://pypi.org/project/fluctlightdb/) · [GitHub](https://github.com/voxmastery/FluctlightDB)

**Not** Postgres or SQLite for your product data. **Not** Qdrant or Pinecone for document search. **Not** a thin memory SDK like mem0 on top of vectors.

**Brain-native** *(optional deep dive)* — the storage model is inspired by how minds remember (linked memories, reinforcement over time), not by rows or cosine similarity alone. See [Manifesto](docs/Manifesto.md) if you want the full design story.

---

## What is this?

FluctlightDB is a **database engine** you run per agent — like SQLite for an app, but the schema and queries are designed for **agent memory**:

| You need to… | Fluctlight API | Plain English |
|--------------|----------------|---------------|
| Save something the agent should remember | `experience(...)` | “User prefers dark mode after yesterday’s session” |
| Find relevant past context from a hint | `activate(cue)` | “What do we know about theme / dark mode?” |
| Know if a memory is trusted vs guessed | provenance on each record | Official balance from a file beats a chat claim |
| Save to disk | `checkpoint()` | Persist like committing a transaction |

**One agent → one data directory on disk** (same idea as one SQLite file or one Qdrant collection). Install with pip, embed in your process, or run as a server for teams.

---

## Who is this for?

- **Developers** building coding agents, support bots, research assistants, game NPCs  
- **Teams** that need memory to survive restarts, not just the current LLM context window  
- **Enterprises** that need **source-of-truth ranking** (audit logs, CRM, ledger) vs model hallucination  

If you only need “stuff the user said last week” in a vector index, mem0 or a vector DB may be enough. If you need a **first-class database for how agents remember and grow**, use FluctlightDB.

---

## How it compares

| | **Postgres / SQLite** | **Vector DB** (Qdrant, Pinecone) | **Memory SDK** (mem0, etc.) | **FluctlightDB** |
|---|----------------------|----------------------------------|----------------------------|------------------|
| **Category** | General-purpose SQL | Similarity search | Chat → extract → embed → search | **Agent memory engine** |
| **Stores** | Tables and rows | Vectors + JSON payload | Facts / message summaries | **Linked memories** with context |
| **Query style** | SQL | nearest neighbor | embed query, top-k | **cue → recall** (meaning, not just match score) |
| **Trusted vs chat** | You design it | You design it | Often LLM-labeled | **Built in** (verified sources rank higher) |
| **Typical install** | driver + server | client + server | pip SDK + vector backend | **`pip install fluctlightdb`** |

---

## Quick start (~2 minutes)

**Recommended:** run inside your Python app — no separate server, like `sqlite3`.

On Debian/Ubuntu/Fedora, use a venv ([PEP 668](https://peps.python.org/pep-0668/) blocks global pip):

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

---

## Choose your path

```
One agent in one process (fastest — start here)
  pip install "fluctlightdb[native]"
  brain = connect("/path/to/agent-data")

Several agents or a shared server (platform / ops team)
  pip install fluctlightdb
  Docker or release binary → FluctlightClient over HTTP

Explore data at the terminal
  fluctlight shell  (binary from GitHub Releases)

Work on the Rust engine or CLI
  clone + cargo — see CONTRIBUTING.md
```

### Optional: HTTP client + server

When many services share one agent store (like `qdrant-client` + Qdrant):

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

Release binaries: [GitHub Releases](https://github.com/voxmastery/FluctlightDB/releases). Production: [DEPLOYMENT.md](docs/DEPLOYMENT.md) · [DOCKER.md](docs/DOCKER.md).

---

## Performance (measured)

Linux x86_64, in-process [`prod_bench`](crates/fluctlightdb/tests/prod_bench.rs):

| Path | Latency |
|------|---------|
| **Embedded** (`connect`) | ~**0.002 ms** / recall |
| **HTTP** (`FluctlightClient`, keep-alive) | ~1–5 ms / request on localhost |

On `"wallet balance"`, a verified ledger value ranks above an unverified chat claim. Reproduce: `./scripts/manifesto-audit.sh`.

---

## Documentation

| Doc | Who it's for |
|-----|----------------|
| **[Getting started](docs/GETTING_STARTED.md)** | Paths, storage, FAQ |
| [CLI.md](docs/CLI.md) | Terminal commands (`fluctlight shell`) |
| [DOCKER.md](docs/DOCKER.md) | Container deploy |
| [DEPLOYMENT.md](docs/DEPLOYMENT.md) | Backup, replica, multi-tenant |
| [Manifesto.md](docs/Manifesto.md) | Brain-native design (technical) |
| [openapi.yaml](docs/openapi.yaml) | HTTP API |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Engine development |

---

## Contributing

**Using Fluctlight in an agent?** `pip install fluctlightdb` — no Rust required.

**Changing the engine, CLI, or server?** See [CONTRIBUTING.md](CONTRIBUTING.md). Security: [SECURITY.md](SECURITY.md).

## License

MIT OR Apache-2.0 — see `LICENSE`, `LICENSE-MIT`, `LICENSE-APACHE`.
