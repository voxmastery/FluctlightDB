# FluctlightDB

**A brain-native database for AI agents.** Not a vector database. Not SQL. Not a memory layer like mem0.

Store what your agent **experienced**, recall by **spreading activation** (not just cosine similarity), and separate **verified facts from chat guesses**.

[![PyPI](https://img.shields.io/pypi/v/fluctlightdb)](https://pypi.org/project/fluctlightdb/) · [GitHub](https://github.com/voxmastery/FluctlightDB)

---

## What is this?

**FluctlightDB is a database** — a store you install (`pip install fluctlightdb`), open per agent (`connect(path)`), and query with brain-native verbs (`experience`, `activate`). It is shaped like **biological memory** (engrams, synapses, activation), not like a document index with an LLM wrapper on top.

If you build agents (coding assistants, workers, NPCs, companions), they need memory beyond the current chat window.

| | **Memory layers** (mem0, etc.) | **Vector DB** (Qdrant, Pinecone) | **FluctlightDB** |
|---|-------------------------------|----------------------------------|------------------|
| **What it is** | SDK + extraction pipeline over a vector store | Similarity search engine | **Brain-native database** |
| **Unit stored** | facts / messages (often summarized) | embedding + payload | **engram** (lived episode) |
| **Recall** | embed query → top-k similar | nearest neighbor | **spreading activation** from a cue |
| **Truth** | LLM or metadata flags | payload fields | **provenance** (ledger beats chat) |
| **Growth** | re-ingest / re-summarize | re-index | **sleep, plasticity, maturation** |

**Mental model:** one **brain folder per agent** on disk — like one SQLite file or one Qdrant collection path. Your agent **lives** in that brain; memory is not a sidecar RAG index you bolt on each turn.

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

## Brain-native vs mem0-style memory layers

Tools like **mem0**, Zep, or LangMem are **memory layers**: they extract or summarize chat, embed it, and search a vector store. FluctlightDB is a **database whose model is a brain**:

- **mem0-style:** “remember this fact about the user” → embed → retrieve similar chunks next turn  
- **FluctlightDB:** `experience(...)` writes an engram → `activate(cue)` spreads activation through a graph → verified ledger outranks unverified chat  

You can use both in one stack (mem0 for quick facts, Fluctlight for durable episodic brain state), but they solve different problems. Fluctlight is for agents that need a **persistent mind**, not just a better prompt prefix.

Philosophy and automated checks: [Manifesto.md](docs/Manifesto.md) · `./scripts/manifesto-audit.sh`

---

## How is this different from SQL?

| | SQL | Vector DB | FluctlightDB |
|---|-----|-----------|--------------|
| **Query** | `SELECT … WHERE` | nearest neighbor | **`activate(cue)`** |
| **Best for** | billing, inventory, reports | similarity search | **episodic agent memory + truth** |
| **Install** | driver + server | pip client + server | **`pip install fluctlightdb`** |

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
