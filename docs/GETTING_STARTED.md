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
| **In-process (fast)** | `sqlite3`, `rusqlite` | rare (embedded libs) | **`fluctlightdb_native`** (PyO3) |
| **Client/server** | TCP to Postgres | HTTP/gRPC | `fluctlight serve` (HTTP) |
| **Hot-path latency** | ~0.1 ms | ~2–5 ms (ANN) | **~0.02 ms native**, ~1–5 ms HTTP keep-alive |
| **Best for** | Structured ops data | Similarity search | **Episodic agent memory + truth** |

## 5-minute quick start

### 0. Prerequisite (one-time build)

Fluctlight is written in Rust. **Agent developers** usually only run this once to get the CLI + native Python wheel — not on every project:

```bash
git clone https://github.com/voxmastery/FluctlightDB.git
cd FluctlightDB
cargo build --release          # CLI + server binary
./scripts/install-native.sh    # Python library (like building sqlite extension once)
```

Compare: Qdrant README says `docker pull` or `pip install qdrant-client`; SQLite is already on your system. We’re not published to PyPI yet, so `install-native.sh` is the equivalent.

### 1. Try the REPL (like `psql` / `sqlite3`)

```bash
./target/release/fluctlight shell --local --path /tmp/demo-brain
```

```
fluctlight> experience agent learned user prefers dark mode
fluctlight> recall dark mode
fluctlight> list 5
fluctlight> help
fluctlight> quit
```

### 2. Python agent (recommended — like `import sqlite3`)

```bash
./scripts/install-native.sh
export FLUCTLIGHT_NATIVE=1
python3 << 'PY'
import sys
sys.path.insert(0, "sdks/python")
from fluctlightdb import get_recall_client
c = get_recall_client("/tmp/demo-brain")
print(c.activate("dark mode"))
PY
```

### 3. HTTP API (like Qdrant Docker + REST client)

```bash
./target/release/fluctlight tenant provision myagent --role admin
# follow printed auth.env lines
./target/release/fluctlight serve --path ~/.fluctlight/tenants/myagent/brain
```

Use the Python SDK with `FLUCTLIGHT_SERVE_URL` and `FLUCTLIGHT_API_KEY`.

### 5. Visualize the brain

```bash
./target/release/fluctlight export-viz /tmp/brain-viz.json
# open docs/visual.html in a browser and load the JSON
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
│  Recall every turn (hot path)                           │
│  → fluctlightdb_native  (in-process, default)           │
├─────────────────────────────────────────────────────────┤
│  Writes + multi-tenant                                  │
│  → fluctlight serve (HTTP only)                         │
├─────────────────────────────────────────────────────────┤
│  Force HTTP for recall (debug only)                     │
│  → FLUCTLIGHT_HTTP_RECALL=1                             │
└─────────────────────────────────────────────────────────┘
```

Provision per-agent brain + API key:

```bash
./target/release/fluctlight tenant create agent-42
./target/release/fluctlight tenant provision agent-42 --role admin
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

**Do I write SQL?**  
No. Use `recall`, `experience`, `list`, or the REPL. SQL mental model maps in [CLI.md](CLI.md).

**Can I replace Postgres?**  
Not for relational ops. Use Fluctlight for **what the agent lived and remembers**.

**How do I know what's true?**  
`verified` / `warnings` in the shell, or `verified_context` in the API — ledger/file beats chat.
