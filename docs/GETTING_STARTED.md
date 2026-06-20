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

### 1. Build

```bash
git clone https://github.com/voxmastery/FluctlightDB.git
cd FluctlightDB
cargo build --release
```

### 2. Try the REPL (like `psql`)

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

### 3. Python agent (library call — recommended)

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

### 4. HTTP API (multi-tenant / remote writes)

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

## Which path should I use?

```
┌─────────────────────────────────────────────────────────┐
│  Agent recall every turn (hot path)                     │
│  → fluctlightdb_native  (like sqlite3)                  │
├─────────────────────────────────────────────────────────┤
│  Writes from many services / tenants                    │
│  → fluctlight serve + API key                           │
├─────────────────────────────────────────────────────────┤
│  Debugging / exploring memory                           │
│  → fluctlight shell                                     │
├─────────────────────────────────────────────────────────┤
│  Structured business data (orders, users)               │
│  → keep Postgres — not Fluctlight's job                 │
└─────────────────────────────────────────────────────────┘
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
