# Production threat model

Status: Phase 5 baseline, not an external audit. Production readiness is false until the
machine-readable release gate reports `production_ready: true`.

## Assets and trust boundaries

Protected assets are tenant memories, WAL/checkpoints, API credentials, cluster pepper,
node private keys, control metadata, placement/fence generations, and durability
acknowledgements. Trust boundaries exist at the public TLS proxy, HTTP parser, tenant
authorization layer, data/control RPC mTLS endpoints, filesystem, backup target, and
operator interface.

Operators and the host kernel are trusted. Clients, request bytes, tenant identifiers,
peer network traffic before mTLS authentication, filesystem names, stale primaries, and
restored media are untrusted.

## Threats and required controls

| Threat | Control | Verification |
|---|---|---|
| Request smuggling, malformed framing, slowloris | Hyper framing, body/header limits, absolute timeout, connection cap | `async_serve_boundary`, ignored 100k malformed gate |
| Cross-tenant access or confused deputy | Capability role and tenant binding, canonical tenant paths | `zz_security_review`, `auth_tenant` |
| Key theft/replay | TLS, no plaintext key persistence, revocation/expiry in replicated control state | control and three-node tests |
| Rogue cluster peer | Mutual TLS and certificate fingerprint registry | `control_distributed_cluster` |
| Split brain/stale primary | placement generation, writer epoch and WAL fence identity | placement and subprocess stale-fence tests |
| Lost acknowledged write | WAL fsync before acknowledgement and quorum/all watermarks | idle durability and tenant replication tests |
| Torn/corrupt storage | immutable generations, atomic `CURRENT`, checksummed/contiguous WAL | checkpoint crash matrix and WAL rejection tests |
| Symlink/reparse redirection | no-follow lock open and private files/directories | Linux and Windows platform gates |
| Resource exhaustion | request/body/concurrency limits, load shedding, bounded load gate | async boundary and load gate |
| Malicious/corrupt snapshot | length/SHA-256 verification and staged activation | replication checkpoint tests |
| Rollback or incompatible upgrade | exact version pin, pre-upgrade backup, rolling compatibility gate | operations runbook |
| Disk exhaustion or slow storage | no acknowledgement after injected ENOSPC; timeout/health alerting | WAL fault simulation gates |

## Residual risks

No third-party penetration test or cryptographic protocol audit has been completed.
Host compromise, compromised operators, kernel/filesystem lies after successful fsync,
traffic-analysis leakage, and denial of service above configured proxy capacity remain
out of scope. Backups require independent encryption and access controls. Fault injection
must never be enabled in a production process.
