# CORTEX — Coordination-Optimal Replay-Tested Engram eXecution

**Status:** Research doctrine + first simulation slice (`feature = "cortex-sim"`)  
**Date:** 2026-07-22  
**Replaces:** speculative MYELIN / THM crypto doctrines (removed from the tree)

## Motive (FluctlightDB)

FluctlightDB is **SQLite for what agents learn**: a third data model
(`experience` / `activate` / provenance / consolidation), not SQL rows and not
“vector DB as memory.” Extreme production for this product means fleets of
agents can trust:

1. An **acknowledged** `experience` is never silently lost under quorum policy.
2. A **stale primary** never accepts a write after placement generation advances.
3. **Recall** stays memory-native under declared consistency (primary, bounded-stale, eventual).
4. Failures are **reproducible** in deterministic simulation, not “works on my laptop.”

CORTEX does **not** claim to delete CAP or PACELC. It claims something stronger
and honest: every extreme-production correctness property is either
**CALM-local** (coordination-free) or **DST-proven** under injected faults, with
production readiness remaining fail-closed until ops evidence exists.

## Research grounding

| Source | What CORTEX takes |
|--------|-------------------|
| FoundationDB / Antithesis DST | Abstract clock/network/disk/RNG; seed-reproducible swarm faults |
| TigerBeetle | Mission-critical storage contracts; simulation-first safety culture |
| CALM (Hellerstein) | Monotone problems can be coordination-free; non-monotone need coordination |
| PACELC (Abadi) | Acknowledged mutations choose **PC/EC** (consistency over availability/latency); recall may use declared EL tradeoffs |

## Architecture (unbundled, Fluctlight-native)

```text
Agent API (experience / activate / checkpoint)
        │
        ▼
┌───────────────────┐     ┌────────────────────────────┐
│ Control plane     │     │ Data plane                 │
│ OpenRaft metadata │────▶│ Brain generations + WAL    │
│ tenants, keys,    │     │ quorum / local durability  │
│ placement, fence  │     │ never in Raft log          │
└───────────────────┘     └────────────────────────────┘
        ▲
        │
┌───────┴────────┐
│ CORTEX DST     │  pluggable clock / rng / net / fs
│ seed-reproducible histories + oracles
└────────────────┘
```

Engram payloads **never** enter Raft. Control owns identity and fencing; data
owns durable experience logs.

## CALM memory split

### Monotone / coordination-free

- Follower `activate` / `recall` under **eventual** or **bounded-stale** policy
- Index rebuild from a sealed immutable generation
- Watermark observation that only increases

### Non-monotone / coordinated

- `experience`, death, compact, promote, revoke, key issue
- Require local primary ownership + current fence generation
- Quorum durability: primary + majority of assigned data replicas ACK the exact mutation before success

## Property ledger (falsifiable)

| ID | Property | Oracle |
|----|----------|--------|
| P1 | Zero acknowledged-write loss under `DurabilityPolicy::Quorum` after primary failover | DST history: every `Ok(experience)` appears in survivor’s durable log |
| P2 | No dual-primary accepts across placement generations | Stale fence → reject; only one generation’s primary accepts |
| P3 | Revocation effective on every ready node within 2s of commit | Linearizable auth digest after applied index |
| P4 | Same DST seed → identical event-trace hash | `CortexRuntime::trace_hash` |
| P5 | Torn/corrupt interior WAL never becomes an acknowledged generation | Checkpoint install / replay reject |

## Explicit non-claims

- No magical CAP defeat or “always available and always linearizable under partition.”
- No TEE / enclave as a security substitute for fencing and quorum.
- No threshold-crypto / Shamir “brain encryption” product path (removed).
- No claim that `production_ready` is true without Phase 5 ops evidence
  (TLS termination, backup/restore drill, bootstrap proof, load gate).

## Mapping to code

| Concern | Module |
|---------|--------|
| Control metadata | `crates/fluctlightdb/src/control/` |
| Placement / fence / read policy | `placement.rs` |
| Canonical WAL + identity | `wal.rs` |
| Quorum tenant replication | `replicate.rs` |
| HTTP edge | `serve.rs` |
| DST kernel (this doctrine’s slice) | `cortex_sim/` behind `feature = "cortex-sim"` |

Neuroscience module `cortex.rs` (semantic consolidation) is unrelated; CORTEX
the doctrine lives in `cortex_sim` to avoid the name collision.

## First slice

A single-threaded deterministic runtime with:

- `CortexClock`, `CortexRng`, `CortexNet` (partitionable), `CortexFs` (logical)
- Three simulated nodes running placement fencing
- Scenario: issue placement → experience on primary → partition → promote → reject stale write → activate on new primary
- Seed replay equality

Later work (out of slice): Flow-style actor rewrite of the whole server,
Antithesis hypervisor integration, multi-region clocks.
