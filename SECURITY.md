# Security Policy

## Audit & CVE history

| Item | Status |
|------|--------|
| Third-party security audit | **None to date** (July 2026) |
| Published CVEs against FluctlightDB | **None** |
| GitHub Security Advisories | [View advisories](https://github.com/voxmastery/FluctlightDB/security/advisories) — none published yet |
| **Supply-chain CI** | `cargo audit` + `cargo-deny` on every `main` push (see [`deny.toml`](deny.toml)) |

### Known dependency advisories (tracked, not ignored)

CI `cargo audit` currently reports (as of July 2026):

| Crate | Issue | Mitigation |
|-------|-------|------------|
| `bincode` 1.3 | Unmaintained (RUSTSEC-2025-0141) | Evaluate migration to bincode 2 or alternative |
| `memmap2` 0.9.10 | Unsound advisory (RUSTSEC-2026-0186) | Bump when fixed upstream |

**Resolved:** `pyo3` upgraded to **0.29** in `fluctlight-py` (RUSTSEC-2025-0020, RUSTSEC-2026-0177).

The supply-chain job may show warnings until bincode/memmap2 are resolved.

FluctlightDB has **not** undergone a professional penetration test or formal security audit. Production multi-tenant HTTP serve and governance APIs exist but are documented as **not production-hardened** — see [docs/STABILITY.md](docs/STABILITY.md).

**Researchers & auditors:** We welcome coordinated review. Report findings via the process below; for audit sponsorship or extended disclosure windows, contact the maintainer through GitHub.

## Supported versions

Security fixes are applied to the latest release on the `main` branch. We recommend running the current `main` or the latest tagged release.

## Reporting a vulnerability

**Please do not open a public GitHub issue for security vulnerabilities.**

Report privately by opening a [GitHub Security Advisory](https://github.com/voxmastery/FluctlightDB/security/advisories/new) or emailing the maintainers through GitHub (repository owner contact).

Include:

- Description of the issue and potential impact
- Steps to reproduce
- Affected versions or commits
- Suggested fix (if any)

We aim to acknowledge reports within 72 hours and will coordinate disclosure once a fix is available.

## Secrets and deployment hygiene

- Never commit API keys, brain snapshots, or `auth.env` files.
- Use `/etc/fluctlight/auth.env` (mode `600`) or environment variables in production.
- Rotate credentials if they were ever exposed in logs, chat, or version control.
- The example files `systemd/auth.env.example` and `systemd/environment.example` contain placeholders only.

## Scope

In scope: FluctlightDB core (`crates/`), CLI, HTTP serve, Python SDK, and documented deployment paths.

Out of scope: Third-party agent applications that embed FluctlightDB unless the vulnerability is in FluctlightDB itself.
