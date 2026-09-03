# Phase 5 production operations runbook

## Release decision

Run from a clean release candidate checkout:

```bash
FLUCTLIGHT_SERVER_MODE=production \
FLUCTLIGHT_REQUIRE_AUTH=true \
FLUCTLIGHT_TLS_TERMINATED=true \
FLUCTLIGHT_BACKUP_VERIFIED=true \
FLUCTLIGHT_RESTORE_DRILL_VERIFIED=true \
FLUCTLIGHT_CONTROL_BOOTSTRAP_REPORT=/secure/control-bootstrap-proof.json \
FLUCTLIGHT_WINDOWS_GATE_REPORT=/secure/windows-gate.json \
python3 scripts/phase5_release_gate.py --report target/phase5-release-report.json
```

For the first control-plane node, provide the one-time platform secret only through
`fluctlight serve --bootstrap-secret-file /secure/bootstrap.secret` (the file must be
mode `0600` and is removed after the Raft commit) or pipe it to
`--bootstrap-secret-stdin`. Never put the secret in an environment variable or command
argument. The archived bootstrap proof must record a positive Raft revision, successful
control-plane authorization, rejected reuse, and confirmation that no plaintext was
persisted. `FLUCTLIGHT_API_KEYS` and local `auth.db` credentials are standalone-only and
are ignored whenever a distributed control node is attached.

Do not deploy unless the report exits zero and contains `"production_ready": true`.
Archive the report, load report, exact binary digest, configuration digest, and restore
drill evidence with the change record. Never set `FLUCTLIGHT_ENABLE_FAULT_INJECTION` in
production.

## Deployment and rolling upgrade

1. Confirm quorum health, zero unexpected replication lag, free disk above 30%, and a
   restore-tested backup.
2. Drain one follower, stop it, install the exact pinned binary, restart it, and wait for
   snapshot/WAL catch-up to the committed watermark.
3. Repeat for the other follower. Transfer leadership away from the final old-version
   node, then upgrade it.
4. Abort if a node rejects the existing v4 checkpoint/WAL, fence generations diverge,
   revocation revision stops advancing, or catch-up exceeds the maintenance objective.
5. Run read/write/recall and key-revocation canaries before undraining traffic.

Never downgrade after a writer has emitted a format unsupported by the old binary.

## Alerts and immediate actions

| Signal | Page threshold | First action |
|---|---|---|
| `/ready` false | 2 consecutive probes | remove node from traffic; inspect control revision/fence |
| durable watermark lag | exceeds configured RPO for 60 s | stop primary movement; inspect replica disk/network |
| fsync/write error or disk >90% | any / sustained | stop writes, add capacity; do not claim acknowledgements |
| stale-primary rejection | any outside a planned failover | verify partition and placement generation |
| WAL corruption/gap | any | quarantine files and node; preserve evidence; restore/catch up |
| auth/revocation divergence | one revision beyond propagation SLO | block affected key at proxy and repair control quorum |
| p99 or error-rate gate breach | two windows | shed load, reduce concurrency, inspect slow clients |

## Failover and partition

Fence the old primary before promotion. Promote only a candidate at the committed
watermark using the expected placement generation. Verify the old primary returns
`placement_unavailable` and does not mutate local state. Do not force two primaries.

## Corruption and recovery

1. Stop the affected process and copy the brain directory, WAL, logs, and release report.
2. Do not edit or skip an interior corrupt WAL record. Quarantine the node.
3. Restore the last verified checkpoint, then replay only contiguous verified WAL.
4. Compare tenant UUID, writer epoch, fence generation, committed watermark, and recall
   canaries before rejoining as a read-only follower.
5. Install a fresh authenticated snapshot if no valid contiguous recovery exists.

## Backup/restore drill

Take backups from a published immutable generation plus required WAL. Restore to an
isolated path as read-only, verify format and manifests, replay WAL, validate tenant and
fence identity, and compare canary recalls. Record recovery time and recovered watermark.

## Load gate

Against a staging instance with production limits:

```bash
python3 scripts/phase5_load_gate.py \
  --url https://staging.example.com/live --requests 10000 --concurrency 64 \
  --slow-clients 32 --minimum-success-rate .995 --maximum-p99-ms 500 \
  --maximum-recovery-ms 1000 > target/phase5-load-report.json
```
