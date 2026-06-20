# Industrial deployment (single-host HA)

FluctlightDB v1 targets **single-host industrial agents** with primary + replica, backups, and read scaling — not multi-region managed cloud yet.

## Architecture

```
                    ┌─────────────────┐
   Agent writes ──► │ fluctlight-serve │ :8792  (primary, read/write)
                    │  brain (v4 dir)  │
                    └────────┬─────────┘
                             │ WAL + snapshot
                    ┌────────▼─────────┐
                    │ fluctlight       │
                    │ replicate        │
                    └────────┬─────────┘
                             │
                    ┌────────▼─────────┐
   Agent recall  ──►│ replica serve    │ :8794  (read-only, optional)
   (HTTP fallback)  │  replica/brain   │
                    └─────────────────┘

   Agent hot path ──► fluctlightdb_native (in-process, no HTTP)
```

## Component checklist

| Component | Purpose | Status |
|-----------|---------|--------|
| **Primary serve** | Writes, multi-tenant API | Production v1 |
| **Native Python lib** | Sub-ms recall | Production v1 |
| **Replica + replicate** | Read scaling, failover read path | Production v1 |
| **Backup timer** | `systemd/fluctlight-backup.timer` | Production v1 |
| **FTS5 + HNSW sidecar** | 10k+ engram recall | Production v1 |
| **Multi-region managed** | Hosted SaaS | Roadmap |

## Primary server

Download the latest `fluctlight` binary from [GitHub Releases](https://github.com/voxmastery/FluctlightDB/releases), or build from source:

```bash
cargo build --release   # contributors / operators only
./target/release/fluctlight tenant provision default --role admin
sudo cp systemd/fluctlight-serve.service.d/production.conf /etc/systemd/system/fluctlight-serve.service.d/
sudo systemctl enable --now fluctlight-serve
```

Environment: see `docs/runbooks/tenant-provisioning.md`.

## Replication (read replica)

Terminal 1 — replicate WAL/snapshots:

```bash
FLUCTLIGHT_PRIMARY_BRAIN=~/.fluctlight/tenants/default/brain \
FLUCTLIGHT_REPLICA_DIR=~/.fluctlight/replica \
./scripts/replicate.sh
```

Terminal 2 — serve read-only replica:

```bash
./target/release/fluctlight serve --replica --addr 127.0.0.1:8794 \
  --path ~/.fluctlight/replica/brain
```

Point **read-only** clients at `:8794`. Writes stay on primary `:8792`.

Systemd units: `systemd/fluctlight-replicate.service`, `systemd/fluctlight-replica-serve.service`.

## Backup & restore

```bash
./scripts/fluctlight-backup.sh    # snapshot + sidecar index
./scripts/fluctlight-restore.sh   # restore from backup
```

Runbooks: `docs/runbooks/backup-restore.md`, `docs/runbooks/serve-crash-recovery.md`.

## Industrial agent wiring

```python
# Hot recall — always native when co-located with agent
from fluctlightdb import get_recall_client
client = get_recall_client()  # ~0.02 ms p50

# Writes — HTTP to primary
from fluctlightdb import FluctlightClient
db = FluctlightClient.from_env()
db.experience("user confirmed deployment", context="ops")
```

Env:

```bash
export FLUCTLIGHT_NATIVE=1
export FLUCTLIGHT_SERVE_URL=http://127.0.0.1:8792
export FLUCTLIGHT_API_KEY=...
```

## HTTP performance (keep-alive + lite)

For agents that must use HTTP for reads:

- Enable **keep-alive** in your client (`FluctlightClient` SDK does this)
- Use **`POST /api/v1/activate-lite`** for top-1 recall (~200 bytes vs 6 KB)
- Use **`POST /api/v1/activate-batch`** for multiple cues per round-trip

Still prefer **native** for per-turn recall; HTTP is for cross-process / remote.

## Scale validation

Run the 10k engram industrial recall bar:

```bash
FLUCTLIGHT_SCALE_BENCH=1 ./scripts/longmemeval-scale-bench.py
# target: ≥85% hit rate with FTS5+HNSW sidecar
```

## Monitoring

```bash
curl -s http://127.0.0.1:8792/metrics
```

Key gauges: `fluctlight_activate_ms_avg`, `fluctlight_synapse_count`, connection counters.

## What we don't claim (yet)

- Multi-region active/active
- Managed cloud control plane
- Beating SQLite on raw `LIKE` speed
- Replacing Postgres for relational ops

For those, use the right tool alongside Fluctlight — not instead of it.
