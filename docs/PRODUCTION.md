# Using FluctlightDB in production (today)

FluctlightDB is **beta** (`0.5.x`). This page sets expectations for teams shipping a real agent — not a marketing claim of enterprise maturity.

**Production readiness defaults to false.** For a production deployment, the Phase 5
machine-readable gate must report `production_ready: true`; see
[`runbooks/phase5-production.md`](runbooks/phase5-production.md). A passing test suite
alone is insufficient when TLS, authentication, backup/restore evidence, or a required
platform gate is absent.

**CORTEX** (`feature = "cortex-sim"`) is an experimental deterministic-simulation
correctness kernel for fencing/failover properties. It strengthens confidence in
distributed failure modes; it does **not** flip `production_ready` by itself. See
[`superpowers/specs/2026-07-22-cortex-extreme-production-doctrine.md`](superpowers/specs/2026-07-22-cortex-extreme-production-doctrine.md).

## Recommended for production today

| Use case | Verdict |
|----------|---------|
| **Single agent, embedded brain on disk** (your process owns the `.brain` directory) | **Use `connect_embedded()`** — see [EMBEDDED.md](EMBEDDED.md) |
| **Single-tenant HTTP serve** on localhost or private network with `FLUCTLIGHT_API_KEYS` | OK with auth enabled; see [SECURITY.md](../SECURITY.md) |
| **Multi-tenant shared HTTP serve** | **Experimental** — adversarial tests exist but no third-party security audit |

**Somnus (always on — no user toggle):** wake ticks/experiences are WAL traces only.
Autonomic durability seals run on their own every `FLUCTLIGHT_SOMNUS_SEAL_EVERY_TICKS`
(default 360) via `systems_seal` only — **no semantic sleep prune**, so activate /
benchmark ranking is unchanged. Semantic `sleep()` still seals too. Obsolete gens prune to
`FLUCTLIGHT_SOMNUS_KEEP` (default 3). CLS-native durability — see
[`superpowers/specs/2026-07-25-somnus-cls-durability-doctrine.md`](superpowers/specs/2026-07-25-somnus-cls-durability-doctrine.md).
`FLUCTLIGHT_SOMNUS=0` is a **debug-only** escape to legacy wake checkpoints (not for production).
You do **not** set `FLUCTLIGHT_SOMNUS=1` to enable it.

**Homeostasis (measurement):** `status().homeostasis` reports seal cadence, generation
hygiene, and agent-prompt token estimates. Agent-lane only:
`activate_for_agent_prompt` / `session_boot_context` pack a budgeted prompt slice
(`FLUCTLIGHT_AGENT_ACTIVATE_MAX`, `FLUCTLIGHT_AGENT_PROMPT_TOKEN_BUDGET`) without changing
`activate()` ranking used by benchmarks.

**CortexSchema (Phase A — gates green):** semantic `sleep()` crystallizes durable schemas
into `cortex.schemas`. Default `activate()` unchanged. Opt-in: `activate_with_schemas(cue)`.
Prove: `cargo test -p fluctlightdb --test cortex_schema_gates --test activate_nonregression`.

**CaptureGate (Phase B — gates green):** eligibility tags on experience; sleep captures only
tagged engrams with supersede retention (no episode wipe). Prove:
`cargo test -p fluctlightdb --test capture_gate_gates`.

**Aeterna (Phase C — gates green):** `session_boot_context` / `activate_for_agent_prompt`
lossless id+gist index + `expand_engrams`. Somnus seals on tick **or** WAL pressure.
Prove: `cargo test -p fluctlightdb --test aeterna_gates --test somnus_durability`.

**Lookup-ceiling (impossible bar — frozen):** held-out Person→City→Lang composition
where the (person, lang) pair **never co-occurs in any stored schema statement**;
answer is produced only by query-time slot compose. Prove:
`cargo test -p fluctlightdb --test cls_impossible_held_out_proof`.
Freeze: [`benchmarks/results/cls-impossible-held-out-slot-composition-2026-07-25.json`](../benchmarks/results/cls-impossible-held-out-slot-composition-2026-07-25.json).
Earlier concat-bridge harness remains as a weaker regression:
`cargo test -p fluctlightdb --test cls_lookup_ceiling_proof`.
Does **not** set `production_ready`.

**Ops brain path:** backup/replicate/drill scripts resolve
`serverbrain-v2` before `default` when env is unset (`scripts/resolve-brain.sh`).

## Embedded quick path

```python
from fluctlightdb import connect_embedded

brain = connect_embedded("/data/agent/brain")
brain.turn_begin()
brain.wm_push("User prefers dark mode", context="settings", salience=0.8)
brain.recall("dark mode")          # works before flush (WM lane, 0.5.9+)
brain.turn_end(flush=True)         # durable commit — required for restart survival
brain.checkpoint()
```

Full guide: [EMBEDDED.md](EMBEDDED.md).

## Version pinning

Pin **exact** versions in production. Do not float `>=` in deploy manifests.

```bash
pip install "fluctlightdb[native]==0.5.9" "fluctlightdb-native==0.5.9"
```

Before any upgrade:

1. Read [CHANGELOG.md](../CHANGELOG.md) for the target release.
2. Run your integration tests + `python -m unittest tests.test_quickstart` (or your own recall smoke).
3. Take a brain snapshot / `checkpoint()` before migrating storage.

**0.x policy:** Patch (`0.5.x`) = bug fixes, no intentional stable-API breaks. Minor (`0.6.0`) may add opt-in features. **1.0** will mark a longer stability window after external usage and co-maintainer coverage — not a date commitment.

## Deployment checklist

- [ ] Pin `fluctlightdb` + `fluctlightdb-native` versions
- [ ] Embedded agents: use **`connect_embedded(path)`** ([EMBEDDED.md](EMBEDDED.md))
- [ ] Enable auth for non-localhost HTTP: `FLUCTLIGHT_API_KEYS=tenant:key:role,...`
- [ ] Never commit `auth.env` or brain directories to git
- [ ] Schedule `checkpoint()` / backups for brain directories
- [ ] WM: **`turn_end(flush=True)`** for durable persistence; pre-flush `recall()` hits WM lexically (0.5.9+)
- [ ] Offline lexical recall needs **token overlap** or pass `semantic_vector=` (see [EMBEDDINGS.md](EMBEDDINGS.md))
- [ ] Monitor disk growth; run soak test locally if you expect high write volume ([SOAK_RESULTS.md](SOAK_RESULTS.md))

## What is stable vs experimental

See [STABILITY.md](STABILITY.md). In short:

- **Stable:** `connect*`, `experience()`, `activate()` / `recall()`, `checkpoint()`, on-disk v4 layout
- **Experimental:** Recall Fabric env flags, governance APIs, **multi-tenant auth** (tested, not audited)

## HTTP serve hardening

```bash
# Example: one write key per agent tenant
export FLUCTLIGHT_API_KEYS="agent_a:fld_...:write,agent_b:fld_...:write"
export FLUCTLIGHT_REQUIRE_AUTH=true
fluctlight serve --path /data/brains/default.brain --bind 127.0.0.1:8787
```

Non-localhost bind **requires** API keys (enforced in `serve.rs`). Read-only keys cannot call `/api/v1/experience`.

**Not production-hardened yet:** rate limits are per-tenant best-effort; no WAF, no mTLS, no formal pen test. Report issues via [SECURITY.md](../SECURITY.md).

## Load / soak expectations

CI runs a short soak (`scripts/soak_brain.sh`, ~2k cycles). For your workload, run a longer soak before launch:

```bash
FLUCTLIGHT_SOAK_CYCLES=50000 bash scripts/soak_brain.sh /tmp/soak-brain
```

Record results in your own runbook. Maintainer sample: [SOAK_RESULTS.md](SOAK_RESULTS.md).

## Getting help

- Bugs: [GitHub Issues](https://github.com/voxmastery/FluctlightDB/issues)
- Security: private advisory — [SECURITY.md](../SECURITY.md)
- Co-maintainer / auth review: [SEEKING_AUTH_REVIEWER.md](SEEKING_AUTH_REVIEWER.md)
