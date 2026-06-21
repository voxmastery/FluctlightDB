# Contributing to FluctlightDB

Thanks for your interest in contributing. FluctlightDB is an open-source brain-native memory store for AI agents (MIT licensed).

The **core database is Rust**. The **Python package on PyPI** is the client most agent developers use. Both are in this repo — different roles, different setup.

---

## Who contributes what?

| Goal | Toolchain | Where to work |
|------|-----------|---------------|
| **Use Fluctlight in an agent app** | Python only | `pip install fluctlightdb` — no clone needed |
| **Python SDK** (HTTP client, helpers) | Python 3.9+ | `sdks/python/` |
| **Rust core** (recall, storage, brain logic) | Rust stable | `crates/fluctlightdb/` |
| **CLI + HTTP server + REPL** | Rust stable | `crates/fluctlight-cli/` |
| **Native Python bindings** (in-process recall) | Rust + maturin | `crates/fluctlight-py/` |
| **Docs, runbooks, examples** | Markdown | `docs/`, `README.md` |

You do **not** need Rust to improve docs or the pure-Python SDK. You **do** need Rust for anything in `crates/`.

---

## Git commit identity (maintainers)

Use **one** GitHub-linked identity so the repo shows a single contributor graph. **Voxmastery** is the GitHub username / project brand; copyright is held by **Ganesh S** (see LICENSE).

```bash
git config user.name "Voxmastery"
git config user.email "roppashreeganesh@gmail.com"
# or from repo clone:
./scripts/setup-git-identity.sh
./scripts/setup-git-hooks.sh
```

Do not commit with host-specific emails (e.g. server `@hstgr.cloud` addresses) — GitHub counts those as separate contributors.

---

## Rust setup (core / CLI contributors)

One-time install ([rustup](https://rustup.rs)):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
rustc --version   # should show stable
```

Clone and build:

```bash
git clone https://github.com/voxmastery/FluctlightDB.git
cd FluctlightDB
./scripts/setup-git-hooks.sh
cargo build --release
cargo test --release
```

CI runs the same `cargo test --release` plus `cargo fmt --check` and `cargo clippy`.

### Try your build locally

```bash
# Interactive REPL
./target/release/fluctlight shell --local --path /tmp/test-brain

# One-shot recall
./target/release/fluctlight activate --path /tmp/test-brain "dark mode"

# HTTP server (separate terminal)
./target/release/fluctlight serve --path /tmp/test-brain
```

Integration tests live under `crates/fluctlightdb/tests/` and repo-root `tests/` if present.

---

## Repo layout

```
crates/fluctlightdb/     # Core library — brain, storage, recall, WAL, indexes
  src/brain.rs             # Experience, activate, sleep
  src/serve.rs             # HTTP multi-tenant API
  src/store.rs             # Persistence, checkpoints
  src/activation.rs        # Spreading activation recall
  tests/                   # Integration tests

crates/fluctlight-cli/   # `fluctlight` binary — shell, tenant, export, worker

crates/fluctlight-py/    # PyO3 extension → `fluctlightdb-native` on PyPI

sdks/python/             # Pure Python HTTP client → `fluctlightdb` on PyPI
```

Good first areas (usually self-contained):

- **Docs / examples** — no Rust required
- **Python SDK** — `sdks/python/fluctlightdb/`
- **CLI UX** — `crates/fluctlight-cli/src/shell.rs`
- **Tests** — add cases in `crates/fluctlightdb/tests/`
- **HTTP API** — `serve.rs` + `docs/openapi.yaml`

Harder (read surrounding code first):

- HNSW / sidecar index — `src/index/`
- WAL / replication — `src/wal.rs`, `src/replicate.rs`

---

## Python SDK contributors

Users install from PyPI. To hack on the SDK:

```bash
pip install -e sdks/python
python -c "from fluctlightdb import FluctlightClient; print('ok')"
```

To test against a local server, build the CLI once (`cargo build --release`) and run `fluctlight serve`.

Optional native bindings:

```bash
./scripts/install-native.sh
pip install -e sdks/python
```

---

## Development workflow

1. Fork and create a branch from `main`.
2. Make focused changes; one concern per PR when possible.
3. Before opening a PR (Rust changes):

   ```bash
   cargo fmt --all
   cargo clippy --release --all-targets
   cargo test --release
   ```

4. Update docs if behavior or public API changed.
5. Open a PR — template checklist is in `.github/pull_request_template.md`.

Do not commit secrets, production brain paths, or personal hostnames.

---

## Pull requests

- Describe **what** changed and **why**.
- Link related issues when applicable.
- Ensure CI passes (GitHub Actions on every push to `main`).

---

## Publishing (maintainers)

See [docs/PUBLISHING.md](docs/PUBLISHING.md) for PyPI release steps.

---

## Reporting issues

| Bug in… | Include |
|---------|---------|
| Rust core / CLI | `rustc --version`, OS, steps, `cargo test` output if relevant |
| Python SDK | `pip show fluctlightdb`, Python version, minimal repro |

For security vulnerabilities, see [SECURITY.md](SECURITY.md) — do not open public issues for sensitive reports.

---

## Code of conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md).

---

## License

By contributing, you agree that your contributions will be licensed under the MIT License (see [LICENSE-MIT](LICENSE-MIT)).
