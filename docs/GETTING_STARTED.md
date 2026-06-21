# Getting started with FluctlightDB

**FluctlightDB is a brain-native database for AI agents** — not a vector DB, not SQL, and not a mem0-style memory layer (extract → embed → top-k). Read the [README](../README.md) first; this page goes deeper: paths, comparisons, storage, and FAQ.

## Which path should I use?

```
┌─────────────────────────────────────────────────────────┐
│  One brain folder per agent                           │
│  e.g. /tmp/my-agent-brain or                          │
│       ~/.fluctlight/tenants/<agent_id>/brain/         │
├─────────────────────────────────────────────────────────┤
│  Default — in-process (like sqlite3)                  │
│  pip install "fluctlightdb[native]"                   │
│  from fluctlightdb import connect                     │
├─────────────────────────────────────────────────────────┤
│  Shared / remote / multi-agent                        │
│  pip install fluctlightdb + FluctlightClient (HTTP)   │
│  + Docker or release binary (fluctlight serve)        │
├─────────────────────────────────────────────────────────┤
│  Explore at the terminal                              │
│  fluctlight shell (needs binary from Releases)        │
└─────────────────────────────────────────────────────────┘
```

Provision per-agent brain + API key on a server:

```bash
fluctlight tenant create agent-42
fluctlight tenant provision agent-42 --role admin
# brain: ~/.fluctlight/tenants/agent-42/brain/
```

---

## Quick start

### 1. Embedded brain (recommended)

No server. Rust core runs inside your Python process.

On modern Linux (Debian 12+, Ubuntu 23.04+), use a venv — not `sudo pip` ([PEP 668](https://peps.python.org/pep-0668/)):

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install "fluctlightdb[native]"
```

```python
from fluctlightdb import connect

brain = connect("/tmp/my-agent-brain")
brain.experience("User prefers dark mode", context="settings", salience=0.7)
print(brain.activate("dark mode"))
brain.checkpoint()
```

Read-only hot recall: `get_recall_client(path)`.

Or from this repo: `./scripts/install-python-client.sh` (HTTP client); add `[native]` for embedded.

### 2. HTTP client + server (optional)

Use when several processes share one brain or ops runs the database.

**Docker:**

```bash
docker pull ghcr.io/voxmastery/fluctlightdb:latest
docker run -d -p 8792:8792 \
  -e FLUCTLIGHT_API_KEYS=default:your-secret-key:write \
  -v fluctlight-data:/data \
  ghcr.io/voxmastery/fluctlightdb:latest
```

Use `your-secret-key` as `FLUCTLIGHT_API_KEY` in Python. Details: [DOCKER.md](DOCKER.md).

**Release binary** ([GitHub Releases](https://github.com/voxmastery/FluctlightDB/releases)):

```bash
tar xzf fluctlight-*-linux-x86_64.tar.gz
export FLUCTLIGHT_API_KEYS=default:your-secret-key:write
./fluctlight serve --path ~/.fluctlight/tenants/default/brain
```

> Building from source with `cargo` is for [contributors](../CONTRIBUTING.md) only.

**Python:**

```python
import os

os.environ["FLUCTLIGHT_SERVE_URL"] = "http://127.0.0.1:8792"
os.environ["FLUCTLIGHT_API_KEY"] = "your-key"

from fluctlightdb import FluctlightClient

client = FluctlightClient.from_env()
client.experience("User prefers dark mode", context="settings")
print(client.activate("dark mode"))
```

### 3. REPL (optional — needs server binary)

```bash
fluctlight shell --local --path /tmp/demo-brain
```

```
fluctlight> experience user prefers dark mode
fluctlight> recall dark mode
fluctlight> list 5
fluctlight> quit
```

---

## UX comparison: SQL vs Vector vs Fluctlight

### Mental model

| | **SQL** (Postgres, SQLite) | **Vector DB** (Qdrant, Pinecone) | **FluctlightDB** |
|---|---------------------------|----------------------------------|------------------|
| **Unit of storage** | Row in a table | Point + embedding | **Engram** (lived episode) |
| **Query metaphor** | `SELECT … WHERE` | `search(vector)` | **`activate(cue)`** — spreading activation |
| **Truth** | You add columns | Payload metadata | **Provenance** (ledger vs chat) |
| **Growth** | Schema migrations | Re-index | **Sleep, maturation, stages** |

### Developer UX (from your agent code)

| | SQL | Vector | Fluctlight |
|---|-----|--------|------------|
| **Install** | built-in / pip | `pip install qdrant-client` | **`pip install "fluctlightdb[native]"`** (embedded) or `fluctlightdb` (HTTP) |
| **In-process** | `sqlite3` | rare | **`connect()`** with `[native]` |
| **Client/server** | TCP to Postgres | HTTP/gRPC | optional `fluctlight serve` (HTTP) |
| **Hot-path latency** | ~0.1 ms | ~2–5 ms (ANN) | **~0.002 ms** embedded, ~1–5 ms HTTP (localhost) |
| **Best for** | Structured ops data | Similarity search | **Episodic agent memory + truth** |

### Operator UX (human at a terminal)

| Task | SQL | Vector DB | Fluctlight |
|------|-----|-----------|------------|
| Connect | `psql`, `sqlite3` | curl / SDK / web UI | `fluctlight shell` |
| Browse rows | `SELECT * LIMIT 10` | `scroll --limit 10` | `list 10` |
| Search | `WHERE col LIKE '%x%'` | similarity search | `recall wallet balance` |
| Inspect one | `WHERE id = ?` | get point by id | `get <uuid>` |
| Ground truth | manual `is_verified` column | payload flag | `verified` / `warnings` |
| Scripting | `--json` / CSV | JSON API | `\json on` or Python SDK |
| Dump | `pg_dump` | export collection | `export raw` |

**Fluctlight is closest to `psql` + Qdrant scroll**, but verbs are **brain-native** (`recall`, `experience`, `sleep`) — not SQL syntax. See [CLI.md](CLI.md).

---

## One brain per agent — is one file OK?

**Yes, as a concept** — one logical store per agent, like one SQLite file or one Qdrant collection.

| Store | Physical shape |
|-------|----------------|
| SQLite | `agent.db` (single file) |
| Qdrant local | `./storage/collections/my_agent/` (folder) |
| Fluctlight v4 | `brain/` folder + sidecar index (e.g. `~/.fluctlight/tenants/default/brain/`) |

You **copy/back up that path** like you would `agent.db` or a Qdrant storage dir. Legacy single-file `.flct` still loads; new installs use the v4 folder layout. See [DEPLOYMENT.md](DEPLOYMENT.md) for backup scripts.

---

## Next steps

- [CLI.md](CLI.md) — full command mapping from SQL/vector habits  
- [DEPLOYMENT.md](DEPLOYMENT.md) — replicas, backup, industrial single-host HA  
- [Manifesto.md](Manifesto.md) — why brain-native, not SQL  
- [openapi.yaml](openapi.yaml) — HTTP contract  

---

## FAQ for newcomers

**Is this a vector database?**  
No. Vectors are optional *input*; recall is graph activation + provenance, not pure cosine similarity.

**How is this different from mem0 / Zep / LangMem?**  
Those are **memory layers** — usually LLM extraction + embedding + vector search over messages or facts. FluctlightDB is a **brain-native database**: engrams, spreading activation, sleep/plasticity, and provenance (ledger beats chat) are the storage model, not bolt-on metadata on a vector index.

**Do I need Rust or cargo?**  
No — for agent apps, `pip install fluctlightdb` (or `[native]`) inside a venv is enough. Rust is only for contributors and optional server builds.

**Why does `pip install fluctlightdb` say `externally-managed-environment`?**  
Your OS Python is reserved for system packages (PEP 668). Create a venv (`python3 -m venv .venv && source .venv/bin/activate`) then run `pip install` again. Do not use `--break-system-packages` unless you fully accept the risk to system Python.

**Do I write SQL?**  
No. Use `experience`, `activate`, `list`, or the REPL. SQL mental model maps in [CLI.md](CLI.md).

**Can I replace Postgres?**  
Not for relational ops. Use Fluctlight for **what the agent lived and remembers**.

**How do I know what's true?**  
`verified` / `warnings` in the shell, or `verified_context` in the API — ledger/file beats chat.
