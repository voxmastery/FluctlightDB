# Security Policy

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
