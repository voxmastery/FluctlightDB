# Contributing to FluctlightDB

Thanks for your interest in contributing. FluctlightDB is an open-source brain-native memory store for AI agents (MIT licensed).

## Two audiences

### Python agent developers

Use PyPI — **no clone or Rust required**:

```bash
pip install fluctlightdb
```

Report SDK bugs with your `pip show fluctlightdb` version and Python version.

### Rust / core contributors

1. Fork and clone the repository.
2. Install Rust (stable) and Python 3.10+.
3. Enable git hooks (strips automated co-author trailers from commits):

   ```bash
   ./scripts/setup-git-hooks.sh
   ```

4. Build and test:

   ```bash
   cargo build --release
   cargo test --release
   ```

5. Optional — test the Python SDK from source:

   ```bash
   pip install -e sdks/python
   ./scripts/install-native.sh   # local native wheel for recall tests
   ```

## Development workflow

- Create a feature branch from `main`.
- Keep changes focused; prefer small, reviewable PRs.
- This repo uses `.githooks/` (run `./scripts/setup-git-hooks.sh` once) to strip automated co-author trailers from commits.

- Run `cargo fmt` and `cargo clippy` before opening a PR (Rust changes).
- Add or update tests when behavior changes.
- Update docs (`README.md`, `docs/`) when user-facing behavior changes.
- Do not tell agent developers to run `cargo` unless they are contributing to the Rust core.

## Pull requests

- Describe **what** changed and **why**.
- Link related issues when applicable.
- Ensure CI passes (`cargo test --release`).
- Do not commit secrets, production brain paths, or personal hostnames.

## Publishing (maintainers)

See [docs/PUBLISHING.md](docs/PUBLISHING.md) for PyPI release steps.

## Reporting issues

Use GitHub Issues with:

- Steps to reproduce
- Expected vs actual behavior
- Rust version (`rustc --version`) and OS — for core bugs
- `pip show fluctlightdb` — for SDK bugs

For security vulnerabilities, see [SECURITY.md](SECURITY.md) — please do not open public issues for sensitive reports.

## Code of conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md). Be respectful and constructive in all project spaces.

## License

By contributing, you agree that your contributions will be licensed under the MIT License (see [LICENSE-MIT](LICENSE-MIT)).
