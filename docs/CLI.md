# FluctlightDB CLI

Brain-native memory for agents — not SQL, not a vector DB. This guide maps familiar database CLI patterns to Fluctlight commands.

## Quick start

```bash
# Interactive REPL (tables by default)
fluctlight shell --path ~/.fluctlight/tenants/default/brain

# One-shot JSON (scripting)
fluctlight status
fluctlight activate "wallet balance"
```

When `fluctlight-serve` is running, read-only CLI commands prefer HTTP automatically. Use `--local` in the shell to force opening the brain file directly.

## SQL / vector → Fluctlight mapping

| You want (SQL) | You want (vector DB) | Fluctlight |
|----------------|----------------------|------------|
| `SELECT COUNT(*)` | collection info | `status` |
| `SELECT * LIMIT 10` | scroll | `list 10` (in shell) or query `list_engrams` |
| `SELECT * WHERE id = ?` | get point | `get UUID` |
| `WHERE content LIKE '%x%'` | similarity search | `recall CUE` |
| `INSERT` | upsert | `experience CONTENT` |
| `DELETE` | delete point | `forget UUID` (shell, `--writable`) |
| `EXPLAIN` | — | `preplay GOAL` |
| verified column | payload metadata | `verified` / `warnings` |

## Interactive shell

```bash
fluctlight shell [--path BRAIN] [--writable] [--local]
```

Example session (agent wallet memory):

```
fluctlight> status
fluctlight> list 10
fluctlight> recall wallet balance
fluctlight> verified
fluctlight> warnings
fluctlight> preplay wallet balance 3
fluctlight> stage
fluctlight> \json on
fluctlight> quit
```

### REPL commands

| Command | Description |
|---------|-------------|
| `status` | Stage, engram count, synapses, pressure |
| `list [N]` | Browse engrams (paginated) |
| `get UUID` | Full engram detail |
| `recall CUE` | Spreading activation recall |
| `complete CUE` | Pattern completion |
| `verified` | Ledger/file ground truth |
| `warnings` | Unverified factual chat claims |
| `rag DOC [CHUNK]` | RAG chunks by document |
| `preplay GOAL [STEPS]` | Planning / activation path |
| `stage` | CLS maturation metrics |
| `forget UUID` | Remove engram (requires `--writable`) |
| `export viz\|graph\|raw [FILE]` | Export brain snapshot |
| `\json on\|off` | Toggle table vs JSON output |

## HTTP query API

POST `/api/v1/query` with JSON body:

```json
{"query": {"op": "list_engrams", "page": 0, "page_size": 20}}
```

Operations:

- `list_engrams` — browse all
- `list_verified` — verified facts only
- `list_unverified` — unverified factual claims
- `get_engram` — by UUID
- `search_hybrid` — recall with optional vector
- `search_by_rag` — doc/chunk lookup
- `stats` — brain counters
- `forget` / `forget_before` — admin write

## Batch activate (agents)

POST `/api/v1/activate-batch`:

```json
{
  "batch": [
    {"cue": "wallet balance"},
    {"cue": "redis timeout", "semantic_vector": [0.9, 0.1]}
  ]
}
```

Python SDK:

```python
from fluctlightdb import FluctlightClient
client = FluctlightClient.from_env()
client.activate_batch([{"cue": "wallet balance"}, {"cue": "my balance"}])
```

## Access modes (SQL vs Fluctlight)

| Store | Typical access | Process model |
|-------|----------------|---------------|
| **SQLite** | In-process library (`sqlite3`, `rusqlite`) | Same process as your app — no HTTP |
| **PostgreSQL** | TCP client (`psql`, `libpq`) | Separate server process — not HTTP |
| **Qdrant / Weaviate** | HTTP or gRPC API | Separate server — network hop every query |
| **Fluctlight `worker`** | JSON lines on stdin/stdout | **In-process** — brain loaded once, sub-ms recall |
| **Fluctlight `serve`** | HTTP on `127.0.0.1:8792` | Separate process — good for multi-tenant + writes |
| **Fluctlight CLI one-shot** | Subprocess per command | Slow (~50ms) — avoid in agent hot loops |

**Agent hot path (recommended):** `FLUCTLIGHT_WORKER=1` or `FLUCTLIGHT_EMBEDDED=1` → persistent `fluctlight worker`.

```bash
# Manual worker (one line per request)
echo '{"op":"activate","cue":"wallet balance"}' | fluctlight worker --path ~/.fluctlight/tenants/default/brain
```

Python:

```python
from fluctlightdb import get_worker
w = get_worker()
w.activate("wallet balance")  # sub-ms after first call
```

## Environment variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `FLUCTLIGHT_ACTIVATE_CACHE` | `1` | LRU cache for repeat cues |
| `FLUCTLIGHT_CACHE_TTL_SECS` | `60` | Cache TTL |
| `FLUCTLIGHT_CANDIDATE_CAP` | `128` | Max engrams scored per activate |
| `FLUCTLIGHT_SEPARATION_GATE` | `1` | Block near-duplicate chat ingest |
| `FLUCTLIGHT_WORKER` | `0` | Agent apps: persistent in-process worker |
| `FLUCTLIGHT_EMBEDDED` | `0` | Same as `FLUCTLIGHT_WORKER=1` (alias) |

## Visualization

- `fluctlight export-viz` → JSON for [docs/visual.html](visual.html)
- `fluctlight export-graph` → synapse graph
- `fluctlight export-raw` → full engrams + synapses (backup)
