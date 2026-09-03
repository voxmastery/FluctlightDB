# API stability policy

FluctlightDB is **beta** (`Development Status :: 4 - Beta` on PyPI). We ship fast, but we separate **integrator-facing contracts** from **experimental internals**.

## Stable (semver applies)

These are the APIs agent developers should pin to. Breaking changes require a **major** version bump and a migration note in the release.

| Surface | Symbols | Notes |
|---------|---------|-------|
| **Python SDK** | `connect_embedded`, `connect`, `connect_agent`, `connect_index`, `connect_chorus`, `connect_project`, `FluctlightBrain` | Primary entry points |
| **Core operations** | `experience()`, `activate()`, `checkpoint()`, `recall()` | Documented in README and paper |
| **Project brains** | `fluctlight-project` CLI, handoffs, MCP memory tools | Multi-agent monorepo path |
| **HTTP API** | `/api/v1/experience`, `/api/v1/activate`, `/api/v1/checkpoint` | See `docs/openapi.yaml` |
| **On-disk layout** | v4 brain directory (`manifest.json`, `hippocampus/`, `recall_index.sqlite`) | Upgrades are forward-compatible via `legacy_*` readers |

**Patch releases (0.5.x):** bug fixes, benchmark cert updates, wheel/CI fixes — no intentional API breaks.

**Minor releases (0.x.0):** new optional methods, new env flags default-off, new lanes behind explicit `connect_*` entry points.

## Experimental (may change without major bump)

| Area | Examples | How to tell |
|------|----------|-------------|
| **Neuroscience-named modules** | `amygdala`, `dentate`, `hippocampus`, `prefrontal`, `muon`, `tau`, `chorus`, `prism`, `spectrum` | Rust `crates/fluctlightdb/src/*.rs` — not re-exported as stable Python API |
| **Recall Fabric** | `FLUCTLIGHT_FABRIC=1`, lattice, phase_parse, chronos | Opt-in env flag; off by default |
| **Governance / snapshot** | `brain_snapshot`, `governance`, `retention_policy`, `wm_ring` | New in 0.5.x; API may evolve |
| **Auth / multi-tenant** | `auth.rs`, `tenant.rs`, `auth_store.rs` | **Adversarial tests in CI** (`tests/auth_tenant.rs`, `tests/serve_integration.rs`); **not** third-party audited for production multi-tenant |
| **Distributed control** | Cargo feature `distributed` (OpenRaft, placement, quorum replication) | Opt-in; Phase 5 readiness remains fail-closed without ops evidence |
| **CORTEX simulation** | Cargo feature `cortex-sim`, doctrine in `docs/superpowers/specs/2026-07-22-cortex-extreme-production-doctrine.md` | Deterministic fencing/failover oracles — **not** a production readiness claim |
| **Benchmark harnesses** | `benchmarks/*.py`, frozen JSON filenames | Metrics are frozen claims; harness flags change |

## Version alignment

| Package | Role |
|---------|------|
| `fluctlightdb` | Pure Python SDK on PyPI |
| `fluctlightdb-native` | Optional Rust extension (`pip install "fluctlightdb[native]"`) |

Keep versions in sync: `sdks/python/pyproject.toml` and `crates/fluctlight-py/pyproject.toml` must match on release. Native uses **abi3** (`cp39` tag) so one manylinux wheel covers Python **3.9–3.13**.

## Frozen benchmark claims

Headline numbers in README map to **`benchmarks/results/paper-2026-07-09.json`**. Changing recall code that affects published metrics requires re-running harnesses and updating the freeze file before changing README/paper.

Reproduce LoCoMo locally:

```bash
make reproduce-locomo
# or: bash scripts/reproduce-locomo.sh
```

## Reporting breakage

If a **stable** API changes in a patch/minor release, file a [GitHub issue](https://github.com/voxmastery/FluctlightDB/issues) with the pinned version and minimal repro. We treat unintended breaks as release-blocking bugs.
