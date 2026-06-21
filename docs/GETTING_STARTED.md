# Getting started with FluctlightDB

New here? This page compares **how it feels** to use Fluctlight vs SQL vs vector DBs, and gives a 5-minute path to a working agent memory.

## UX comparison: SQL vs Vector vs Fluctlight

### Mental model

| | **SQL** (Postgres, SQLite) | **Vector DB** (Qdrant, Pinecone) | **FluctlightDB** |
|---|---------------------------|----------------------------------|------------------|
| **Unit of storage** | Row in a table | Point + embedding | **Engram** (lived episode) |
| **Query metaphor** | `SELECT … WHERE` | `search(vector)` | **`recall(cue)`** — spreading activation |
| **Truth** | You add columns | Payload metadata | **Provenance** (ledger vs chat) |
| **Growth** | Schema migrations | Re-index | **Sleep, maturation, stages** |

### Operator UX (human at a terminal)

| Task | SQL | Vector DB | Fluctlight |
|------|-----|-----------|------------|
| Connect | `psql`, `sqlite3` | `curl` / SDK / web UI | `fluctlight shell` |
| Browse rows | `SELECT * LIMIT 10` | `scroll --limit 10` | `list 10` |
| Search | `WHERE col LIKE '%x%'` | similarity search | `recall wallet balance` |
| Inspect one | `WHERE id = ?` | get point by id | `get <uuid>` |
| Ground truth | manual `is_verified` column | payload flag | `verified` / `warnings` |
| Scripting | `--json` / CSV | JSON API | `\json on` or Python SDK |
| Dump | `pg_dump` | export collection | `export raw` |

**Fluctlight is closest to `psql` + Qdrant scroll**, but verbs are **brain-native** (`recall`, `experience`, `sleep`) — not SQL syntax. See [CLI.md](CLI.md).

### Developer UX (from your agent code)

| | SQL | Vector | Fluctlight |
|---|-----|--------|------------|
| **Install** | built-in / pip | `pip install qdrant-client` | **`pip install fluctlightdb`** |
| **Client/server** | TCP to Postgres | HTTP/gRPC | `fluctlight serve` (HTTP) |
| **In-process (fast)** | `sqlite3` | rare | optional **`fluctlightdb-native`** |
| **Hot-path latency** | ~0.1 ms | ~2–5 ms (ANN) | **~0.02 ms native**, ~1–5 ms HTTP keep-alive |
| **Best for** | Structured ops data | Similarity search | **Episodic agent memory + truth** |

## 5-minute quick start

### 1. Install the Python SDK (no Rust required)

```bash
pip install fluctlightdb
```

Same as `pip install qdrant-client` — your agent project only needs Python.

### 2. Start a FluctlightDB server

Like Qdrant: **Docker** or a **release binary** — no Rust required.

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

### 3. Python agent (recommended)

```python
import os

os.environ["FLUCTLIGHT_SERVE_URL"] = "http://127.0.0.1:8792"
os.environ["FLUCTLIGHT_API_KEY"] = "your-key"

from fluctlightdb import FluctlightClient

client = FluctlightClient.from_env()
client.experience("agent learned user prefers dark mode", context="settings")
print(client.activate("dark mode"))
```

### 4. Optional — in-process recall (fastest)

When prebuilt wheels exist for your platform:

```bash
pip install "fluctlightdb[native]"
```

```python
from fluctlightdb import get_recall_client

brain = get_recall_client("~/.fluctlight/tenants/myagent/brain")
print(brain.activate("dark mode"))
```

### 5. Try the REPL (needs server binary)

```bash
fluctlight shell --local --path /tmp/demo-brain
```

```
fluctlight> experience agent learned user prefers dark mode
fluctlight> recall dark mode
fluctlight> list 5
fluctlight> quit
```

## One brain per agent — is one file OK?

**Yes, as a concept** — one logical store per agent, like one SQLite file or one Qdrant collection.

| Store | Physical shape |
|-------|----------------|
| SQLite | `agent.db` (single file) |
| Qdrant local | `./storage/collections/my_agent/` (folder) |
| Fluctlight v4 | `~/.fluctlight/tenants/default/brain/` (folder + sidecar index) |

You **copy/back up that path** like you would `agent.db` or a Qdrant storage dir. See [DEPLOYMENT.md](DEPLOYMENT.md) for backup scripts.

## Which path should I use?

```
┌─────────────────────────────────────────────────────────┐
│  One brain directory per agent                          │
│  ~/.fluctlight/tenants/<agent_id>/brain/                │
├─────────────────────────────────────────────────────────┤
│  Agent code (pip install fluctlightdb)                  │
│  → FluctlightClient (HTTP) — works everywhere           │
├─────────────────────────────────────────────────────────┤
│  Optional hot recall                                    │
│  → pip install fluctlightdb-native                      │
├─────────────────────────────────────────────────────────┤
│  Writes + multi-tenant                                  │
│  → fluctlight serve (release binary or your deployment) │
└─────────────────────────────────────────────────────────┘
```

Provision per-agent brain + API key:

```bash
fluctlight tenant create agent-42
fluctlight tenant provision agent-42 --role admin
# brain: ~/.fluctlight/tenants/agent-42/brain/
```

## Next steps

- [CLI.md](CLI.md) — full command mapping from SQL/vector habits  
- [DEPLOYMENT.md](DEPLOYMENT.md) — replicas, backup, industrial single-host HA  
- [Manifesto.md](Manifesto.md) — why brain-native, not SQL  
- [openapi.yaml](openapi.yaml) — HTTP contract  

## FAQ for newcomers

**Is this a vector database?**  
No. Vectors are optional *input*; recall is graph activation + provenance, not pure cosine similarity.

**Do I need Rust or cargo?**  
No — for agent apps, `pip install fluctlightdb` is enough. Rust is only for contributors and optional server builds.

**Do I write SQL?**  
No. Use `recall`, `experience`, `list`, or the REPL. SQL mental model maps in [CLI.md](CLI.md).

**Can I replace Postgres?**  
Not for relational ops. Use Fluctlight for **what the agent lived and remembers**.

**How do I know what's true?**  
`verified` / `warnings` in the shell, or `verified_context` in the API — ledger/file beats chat.
