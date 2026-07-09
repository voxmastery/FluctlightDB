# Changelog

All notable changes to this project are documented here.

Format based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).  
Versioning follows [Semantic Versioning](https://semver.org/) where practical.

**Also see:** [GitHub Releases](https://github.com/voxmastery/FluctlightDB/releases) (binaries, Docker, release notes).

---

## [Unreleased]

---

## [0.5.8] - 2026-07-09

### Fixed
- **`connect_agent()` quickstart broken**: fast ingest + vector-fast skipped graph/synapse wiring when `semantic_vector` was omitted — recall returned empty hits with no error. Fast vector path now requires an explicit vector; `connect_agent()` no longer sets `FLUCTLIGHT_VECTOR_FAST` by default.
- README 30-second example uses lexical cue `dark mode` (offline path) and recalls after `turn_end(flush=True)` so WM is committed.
- Doc snippets in `hub/paper/README.md`, `sdks/python/README.md`, `docs/EMBEDDINGS.md`, `docs/ONBOARDING.md` aligned to the same rules (lexical overlap + flush-before-recall for WM).

### Added
- CI: `tests.test_quickstart` guards README quickstart on native wheel build.
- CI: expanded doc-snippet regression tests (paper card, SDK README, EMBEDDINGS, ONBOARDING).

---

## [0.5.7] - 2026-07-09

### Added
- Jepsen-style chaos harness: `tests/chaos_jepsen.rs`, `scripts/jepsen-chaos.sh`, CI `chaos-jepsen`
- [docs/LEADERBOARD.md](docs/LEADERBOARD.md) — public results policy (no neutral agent-memory registry)

- BEAM retrieval smoke harness: `benchmarks/beam_eval.py`, `make reproduce-beam-smoke`
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
- Paper freeze `benchmarks/results/paper-2026-07-09.json`; LoCoMo cert **99.0%**

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

- LoCoMo CHORUS evidence recall certified **99.0%** — July 2026
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
