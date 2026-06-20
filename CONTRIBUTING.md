# Contributing to FluctlightDB

Thanks for your interest in contributing. FluctlightDB is an open-source brain-native memory store for AI agents (MIT licensed).

## Getting started

1. Fork and clone the repository.
2. Install Rust (stable) and Python 3.10+.
3. Build and test:

   ```bash
   cargo build --release
   cargo test --release
   ```

4. Optional — Python SDK and native extension:

   ```bash
   ./scripts/install-native.sh
   pip install -e sdks/python
   ```

## Development workflow

- Create a feature branch from `main`.
- Keep changes focused; prefer small, reviewable PRs.
- Run `cargo fmt` and `cargo clippy` before opening a PR.
- Add or update tests when behavior changes.
- Update docs (`README.md`, `docs/`) when user-facing behavior changes.

## Pull requests

- Describe **what** changed and **why**.
- Link related issues when applicable.
- Ensure CI passes (`cargo test --release`).
- Do not commit secrets, production brain paths, or personal hostnames.

## Reporting issues

Use GitHub Issues with:

- Steps to reproduce
- Expected vs actual behavior
- Rust version (`rustc --version`) and OS

For security vulnerabilities, see [SECURITY.md](SECURITY.md) — please do not open public issues for sensitive reports.

## Code of conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md). Be respectful and constructive in all project spaces.

## License

By contributing, you agree that your contributions will be licensed under the MIT License (see [LICENSE-MIT](LICENSE-MIT)).
