# Maintainer & project health

FluctlightDB is **early-stage open source**. This page states facts reviewers and adopters ask about — without overselling maturity.

## Bus factor

| Fact | Status |
|------|--------|
| Active maintainers | **1** (solo) |
| Git contributors (all time) | **Voxmastery** (~116 commits) + **Ganesh** (1 commit) — same person, two Git identities before identity normalization |
| Copyright holder | Ganesh S (`voxmastery@ambugo.tech`) |
| Public GitHub org | [voxmastery](https://github.com/voxmastery) |

**Risk:** If the sole maintainer is unavailable, releases, security fixes, and review throughput stop until a co-maintainer is onboarded.

**Mitigation in place:**

- MIT license — fork and continue without permission
- Open harnesses + frozen benchmark JSON — numbers are contestable without maintainer involvement
- CI on every `main` push (`cargo test`, Python SDK, native wheel smoke)
- Documented release process ([docs/PUBLISHING.md](docs/PUBLISHING.md))

**Not in place yet:**

- Co-maintainer or steering committee
- Corporate backing or paid support SLA
- Foundation governance (CNCF, Apache, etc.)
- Named informal reviewers for auth/storage PRs (**seeking volunteers** — comment on [CONTRIBUTING.md](CONTRIBUTING.md) issues if interested)

## Reproduction bounty

**$50 USD gift card** (or regional equivalent) + public credit for the **first verified external reproduction** of each major benchmark. Details and claim form: [docs/REPRODUCIBILITY.md](docs/REPRODUCIBILITY.md) · [issue template](https://github.com/voxmastery/FluctlightDB/issues/new?template=reproduction.yml).

## Supply chain & crash recovery (CI)

| Check | Status |
|-------|--------|
| `cargo audit` + `cargo-deny` | CI job on every `main` push ([`deny.toml`](../deny.toml)) |
| Known advisories | **bincode**, **memmap2** warnings — see [SECURITY.md](SECURITY.md); **pyo3 0.29** ✅ |
| Miri (WAL/store) | CI job (best-effort) |
| WAL fault injection | Unit test: truncated tail + corrupt line recovery (`wal.rs`) |
| Full Jepsen / kill -9 harness | **Done (embedded model)** — `scripts/jepsen-chaos.sh`, `tests/chaos_jepsen.rs`, CI job `chaos-jepsen` |
| Distributed multi-node Raft Jepsen | **N/A** — embedded brain + optional primary→replica WAL sync, not a consensus cluster |

## Becoming a co-maintainer

We want additional maintainers. Practical path:

1. Open issues or PRs in an area you care about (Rust core, Python SDK, benchmarks, docs).
2. Reproduce a frozen benchmark locally and post results (see [docs/REPRODUCIBILITY.md](docs/REPRODUCIBILITY.md)).
3. After several merged PRs and sustained interest, ask in a GitHub Discussion or issue for triage/release access.

Co-maintainers get: triage, merge rights, PyPI trusted publishing (by invitation), release tagging.

## Benchmark & security claims

| Claim type | Verified by maintainer only? | Independent third party? |
|------------|------------------------------|---------------------------|
| LoCoMo 99.0% evidence recall | Yes (frozen cert JSON) | **No published independent reproduction yet** |
| LongMemEval session@8 97.6% | Yes | **No published independent reproduction yet** |
| LongMemEval E2E 97.4% | Yes (locked run, OpenAI cost) | **No** — locked artifact only |
| BEIR SciFact nDCG@10 | Yes | **No published independent reproduction yet** |
| Security audit | N/A | **None** — see [SECURITY.md](SECURITY.md) |

See [docs/REPRODUCIBILITY.md](docs/REPRODUCIBILITY.md) for how to reproduce or dispute numbers.

## Release history

Human-readable notes: [CHANGELOG.md](CHANGELOG.md)  
GitHub Release assets: [releases](https://github.com/voxmastery/FluctlightDB/releases)  
PyPI: [fluctlightdb](https://pypi.org/project/fluctlightdb/) · [fluctlightdb-native](https://pypi.org/project/fluctlightdb-native/)

## Contact

- Bugs & features: [GitHub Issues](https://github.com/voxmastery/FluctlightDB/issues)
- Security: [SECURITY.md](SECURITY.md) (private advisory — do not file public issues for vulns)
- General: repository owner via GitHub
