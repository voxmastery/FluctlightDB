# Changelog

All notable changes to this project are documented here.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).  
Versioning follows [Semantic Versioning](https://semver.org/) where practical.

**Also see:** [GitHub Releases](https://github.com/voxmastery/FluctlightDB/releases) (binaries, Docker, release notes).

---

## [Unreleased]

### Fixed

- **`semantic_top_k` returned the k *worst* candidates.** `BinaryHeap` is a max-heap, so `peek()` yields the largest element; the eviction branch bound it as `min` and popped it, discarding the best candidate on every improvement. Survivors were then sorted by `id.to_string()`, throwing away the ranking. `hybrid_candidates` compounded it by truncating a `HashSet` in hash order — and with seed limits reaching 512 against a cap of 128, that overflow is the normal case. Candidate selection is now rank-ordered end to end. Affects the in-memory index backend (path-less brains); the SQLite sidecar path uses HNSW and was unaffected.
- **HTTP intake corrupted non-ASCII bodies.** `read_http_request` called `String::from_utf8_lossy` on each socket chunk independently, so a multi-byte character spanning a read boundary became replacement characters — silent corruption on the primary ingest path. Framing is now done on bytes and the body decoded once. **Invalid UTF-8 now returns 400** instead of being stored as characters the client never sent.
- **HTTP header names are matched case-insensitively** per RFC 9110 (HTTP/2 requires lowercase). A lowercase `authorization:` was previously dropped — silently downgrading the caller to the default tenant — and a lowercase `content-length:` truncated the body.
- **`agent` and `governance` were dropped on every restart.** `save_v4_dir` wrote 14 segments while `FluctlightBrain` has 17 persistable fields, so unflushed working memory and the compliance audit log silently reset at each open while `audit_log()` kept returning 200 with an empty list. Same bug class as the muon/tau lanes. A new test asserts every segment the manifest declares is actually written.
- **`compact_brain` was under-merging.** `should_merge` scored raw `dg_neurons`, which carry up to six per-engram-unique separator neurons the dentate gyrus fabricates on encode. They can never match, so near-duplicates were capped below the 0.85 threshold. Measured on 120 near-identical engrams: **1 merge before, 7 after.**
- **The separation gate credited fabricated distinctness.** It scored candidates with `peer.separation_index.max(1.0 - jaccard)`; `separation_index` records how well the DG orthogonalised that peer *at its own encode time*, so a peer written while novel carried ~1.0 and donated it to any later near-duplicate. Both consumers now score the clean content code via `derive::content_dg`.

### Added

- **Frozen neuron codec (`FLCT1`).** `NeuronId` used `DefaultHasher`, whose algorithm std explicitly declines to guarantee across releases — yet those ids are written to disk *and* recomputed from token text at query time, so a Rust upgrade would have silently emptied recall on every stored brain. FLCT1 (FNV-1a-64 + fmix64) is pinned by golden vectors and recorded **per brain**, because `serve.rs` pools many brains across threads. The same latent bug is frozen in `semantic::hash_mix` and `shard::shard_for`.
- **Codec drift detection and repair.** Loading a brain re-checks eight known-answer probes. A mismatch means the identity function moved underneath stored data; every engram is queued for re-key, oldest first, and reported on `BrainStatus { neuron_codec, rekey_pending }`. The queue drains during ingest and sleep, or all at once via `rekey_now()`. Drain order is a correctness requirement — `separate_episode` reads a live peer window, so engram *N*'s code depends on 0..*N*-1 as they stood at encode time.
- `SeparationGateResult::best_peer` — a rejection now names the engram it collided with, so a client can repair it via `/api/v1/reconsolidate` instead of receiving an opaque 200 with a nil id.

### Changed

- **Ingest is stricter.** Near-duplicates that previously slipped the gate by inheriting a peer's prior novelty are now rejected (`gate_rejected: true`). Set `FLUCTLIGHT_SEPARATION_GATE=0` to restore the old admission behaviour.
- **`FLUCTLIGHT_CORTEX_WEIGHT` is resolved once per brain at open**, not re-parsed from the environment on every recall. Two brains opened under different settings now keep their own. The mode flags (`FLUCTLIGHT_VECTOR_FAST`, `FLUCTLIGHT_FAST_INGEST`, `FLUCTLIGHT_AGENT_FAST`) deliberately still read the environment on demand, because the SDK sets them at runtime and expects the next call to observe the change.
- `.reproduce-venv/` (11,817 files) and three prebuilt release binaries are no longer tracked; `scripts/reproduce-locomo.sh` recreates the venv from `benchmarks/requirements-reproduce.txt`. Tracked files: 12,272 → 456. Note this does **not** shrink `.git`, which keeps the blobs in history.
- `tests/generated_matrix.rs`: 250 tests that reduced to 17 distinct bodies replaced by 6 parameterized ones covering every `DevStage` and `Region`, plus budget-monotonicity and synapse-downgrade properties the originals never checked.

### Known / unchanged

- Two recall stages are **unreachable at the default neuromodulator posture** and are now pinned by tests rather than left to be discovered: the DA/NE scoring block (guarded by `da > 0.5 || ne > 0.3` against defaults of exactly 0.5 and 0.3) and CA3 Hopfield completion (guarded by `!is_encoding()` against a default ACh of 0.7 ≥ 0.6). Whether to delete, re-gate or re-tune them is an open design decision.
- Configuration is still largely process-global: ~56 `FLUCTLIGHT_*` variables remain ambient and the Python SDK still configures the engine by mutating `os.environ`.

### Migration

- **No on-disk format change.** `format_version` stays 4. A brain written before this release loads unchanged and keeps recalling correctly: it is adopted at `CODEC_LEGACY_STD`, which is the codec it was actually written with, and re-keying is queued rather than forced.
- `life.seg` gained two fields. Because **bincode is not self-describing**, `#[serde(default)]` does *not* backfill a missing trailing field — an explicit legacy reader (`life::read_life_segment`) handles the old four-field shape. Any future segment-shape change needs the same treatment.
- **One-way door:** a brain re-keyed to FLCT1 and then opened by an *older* binary will recall poorly, because that binary computes `DefaultHasher` cues against FLCT1 neurons. Nothing is corrupted, and re-upgrading re-keys it.

### Benchmarks

- **LoCoMo headline corrected to honest raw evidence recall (no expansion):** **96.8% @150 (MiniLM) / 97.0% (mpnet)**, tight-k @5=72.6%/75.1%, @10=80.0%/82.6%, @20=85.6%/87.2%, @50=91.8%/92.4% — native Rust CHORUS first-principles invented stack (salience-gated MaxSim + conjunctive surprisal + evidence-integration fusion). Bench `benchmarks/locomo_engine_maxsim.py`; frozen `benchmarks/results/locomo-invented-stack-engine-2026-07-13.json` (MiniLM), `locomo-mpnet-engine-2026-07-15.json` (mpnet). The prior **99.0%** headline was ±3 neighbor-expansion scoring inflation (`expand_session_neighbors`), not the engine — **deprecated**. Evidence recall ≠ QA accuracy (E2E ≈85% @k=15, retrieval-bound).

---

## [0.5.10] - 2026-07-11

### Fixed
- CI `test` flake: process-wide `test_env::EnvGuard` for all `FLUCTLIGHT_*` mutations (auth, fabric, agent, serve integration).
- Windows `pypi-wheel-smoke`: invoke `python -m pip` (Windows blocks upgrading via `pip.exe`); pip upgrade is best-effort.

---

## [0.5.9] - 2026-07-09

### Added
- **`connect_embedded()`** — production embedded entry: clears serve/auth env pollution, optional `0700` brain dir on Unix.
- **`docs/EMBEDDED.md`** — embedded-first production guide.
- WM lexical fallback in `recall_unified` when episodic lanes are empty (recall before `turn_end(flush=True)`).
- `tests/test_embedded.py` guards.

### Fixed
- Miri CI: split `wal_` and `persistence_roundtrip` test filters (invalid combined filter).

---

## [0.5.8] - 2026-07-09

### Fixed
- **`connect_agent()` quickstart broken**: fast ingest + vector-fast skipped graph/synapse wiring when `semantic_vector` was omitted — recall returned empty hits with no error. Fast vector path now requires an explicit vector; `connect_agent()` no longer sets `FLUCTLIGHT_VECTOR_FAST` by default.
- README 30-second example uses lexical cue `dark mode` (offline path) and recalls after `turn_end(flush=True)` so WM is committed.
- Doc snippets in `hub/paper/README.md`, `sdks/python/README.md`, `docs/EMBEDDINGS.md`, `docs/ONBOARDING.md` aligned to the same rules (lexical overlap + flush-before-recall for WM).
- `cargo-deny` 0.19 config + documented `bincode`/`memmap2` ignores; supply-chain CI.
- HTTP serve test shutdown/env races; soak script path argument.

### Added
- CI: `tests.test_quickstart` guards README quickstart on native wheel build.
- CI: expanded doc-snippet regression tests (paper card, SDK README, EMBEDDINGS, ONBOARDING).
- `docs/PRODUCTION.md`, `scripts/soak_brain.sh`, `docs/SOAK_RESULTS.md`.
- Adversarial auth/tenant tests; auth-reviewer GitHub issue template.

---

## [0.5.7] - 2026-07-09

### Added
- Jepsen-style chaos harness: `tests/chaos_jepsen.rs`, `scripts/jepsen-chaos.sh`, CI `chaos-jepsen`
- [docs/LEADERBOARD.md](docs/LEADERBOARD.md) — public results policy (no neutral agent-memory registry)

- Crash recovery integration tests (`tests/crash_recovery.rs`) — WAL corrupt/truncate/kill simulation
- GitHub label sync workflow (`.github/workflows/sync-labels.yml`)

### Changed
- `cargo audit` passes (warnings only: bincode, memmap2)
- Supply-chain CI no longer `continue-on-error`

---

## [0.5.6] - 2026-07-09

### Added
- Linux **arm64** (`manylinux_2_34_aarch64`) and Windows **arm64** (`win_arm64`) native abi3 wheels on PyPI

### Changed
- CI `pypi-wheel-smoke` covers `ubuntu-24.04-arm` and `windows-11-arm`

---

## [0.5.5] - 2026-07-09

### Fixed
- Multi-platform native wheel publish: correct `dist/` path under `crates/fluctlight-py`

### Added
- PyPI native wheels: Linux x86_64, macOS universal2, Windows x64 (abi3, Python 3.9–3.13)

---

## [0.5.4] - 2026-07-09

### Fixed
- Publish workflow: single `publish-native` job after matrix build (was failing per-platform upload)

---

## [0.5.3] - 2026-07-09

### Added
- macOS universal2 and Windows x64 native wheel build jobs (publish pipeline fixes followed in 0.5.4–0.5.5)

---

## [0.5.2] - 2026-07-09

### Fixed
- PyPI `fluctlightdb-native` abi3 wheel (`cp39-abi3`) — prior 0.5.0 wheel was cp39-only manylinux x86_64
- Removed `PyBuffer` usage incompatible with stable ABI

### Added
- `scripts/reproduce-locomo.sh` + `make reproduce-locomo`
- `scripts/verify-pypi-wheel.sh` + `make test-native-wheel`
- `docs/STABILITY.md`, `docs/EMBEDDINGS.md`
- README restructure (install / API / benchmarks first)
- Paper freeze `benchmarks/results/paper-2026-07-09.json`; LoCoMo cert **99.0%** *(later found to be ±3 neighbor-expansion inflation; superseded by honest 96.8% @150 no-expansion — see [Unreleased] and `locomo-invented-stack-engine-2026-07-13.json`)*

---

## [0.5.1] - 2026-06-25

### Added
- Team sync (`fluctlight-project init --team-sync`), handoff UI, onboarding, VPS + desktop hub

---

## [0.5.0] - 2026-06-25

### Added
- Multi-agent project brains: cross-platform locks, handoff inbox, `fluctlight-project doctor`
- Cursor / Claude / Codex MCP templates and hooks

---

## [0.4.5] - 2026-06-25

### Changed
- Native package version bump for PyPI release alignment

---

## [0.4.3] - 2026-06-21

### Fixed
- Release artifact download and `rustfmt` CI

---

## [0.4.2] - 2026-06-21

### Fixed
- GitHub Release artifact collection before upload

---

## [0.4.1] - 2026-06-20

### Fixed
- CI and PyPI publish workflow

---

## [0.4.0] - 2026-06-20

### Added
- Python SDK published on PyPI (`fluctlightdb`)
- Docs lead with `pip install`

---

## Earlier history

Commits before `v0.4.0` cover Rust core, CHORUS/PRISM benchmarks, arxiv preprint, and initial agent memory API. See `git log` for detail.

**Benchmark milestones (not tied 1:1 to semver tags):**

- LoCoMo evidence recall (honest raw, no expansion): **96.8% @150 (MiniLM) / 97.0% (mpnet)**, @5=72.6%/75.1% tight-k — native Rust CHORUS invented stack, July 2026. (The earlier **99.0%** was ±3 neighbor-expansion inflation, deprecated. Evidence recall ≠ QA accuracy; E2E ≈85% @k=15.)
- LongMemEval session@8 **97.6%**, E2E **97.4%** — July 2026 paper freeze
- BEIR SciFact nDCG@10 **0.645** — July 2026

[0.5.6]: https://github.com/voxmastery/FluctlightDB/compare/v0.5.5...v0.5.6
[0.5.5]: https://github.com/voxmastery/FluctlightDB/compare/v0.5.4...v0.5.5
[0.5.4]: https://github.com/voxmastery/FluctlightDB/compare/v0.5.3...v0.5.4
[0.5.3]: https://github.com/voxmastery/FluctlightDB/compare/v0.5.2...v0.5.3
[0.5.2]: https://github.com/voxmastery/FluctlightDB/compare/v0.5.1...v0.5.2
[0.5.1]: https://github.com/voxmastery/FluctlightDB/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/voxmastery/FluctlightDB/compare/v0.4.5...v0.5.0
[0.4.5]: https://github.com/voxmastery/FluctlightDB/compare/v0.4.3...v0.4.5
[0.4.3]: https://github.com/voxmastery/FluctlightDB/compare/v0.4.2...v0.4.3
[0.4.2]: https://github.com/voxmastery/FluctlightDB/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/voxmastery/FluctlightDB/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/voxmastery/FluctlightDB/releases/tag/v0.4.0
