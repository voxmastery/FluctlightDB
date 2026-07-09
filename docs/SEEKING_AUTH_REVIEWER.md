# Seeking: periodic reviewer for auth / storage PRs

**Not** full co-maintenance — **read-only security + storage review** on a scoped set of paths, ~2–4 hours/month.

## Why

Solo maintainer ([MAINTAINER.md](../MAINTAINER.md)). Auth and WAL/storage changes are high-risk; a second pair of eyes reduces bus-factor and missed isolation bugs.

## Scope (what you'd review)

| Area | Paths |
|------|--------|
| HTTP auth & tenant binding | `crates/fluctlightdb/src/serve.rs`, `auth.rs`, `auth_store.rs`, `tenant.rs` |
| Storage / WAL | `store.rs`, `wal.rs`, `segment.rs` |
| Tests you should expect | `tests/auth_tenant.rs`, `tests/serve_integration.rs`, `tests/chaos_jepsen.rs` |

Out of scope unless you opt in: benchmarks, Python SDK ergonomics, paper figures.

## What we have today

- Adversarial unit tests: cross-tenant HTTP 403, read-role write denial, revoked keys, garbage tokens (`tests/auth_tenant.rs`, `tests/serve_integration.rs`)
- Jepsen-style embedded chaos (SIGKILL mid-write, torn WAL)
- **No** third-party penetration test or formal audit

## Ask

1. Comment on [GitHub issue #TBD](https://github.com/voxmastery/FluctlightDB/issues) (open from maintainer) with your background (Rust, security, databases).
2. Maintainer adds you as optional reviewer on PRs touching the paths above.
3. No SLA — best-effort review within ~1 week of tag.

## Not offering

- Paid compensation (volunteer / academic / portfolio credit only)
- Production security guarantee — review reduces risk, does not certify

Maintainer contact: comment on **[GitHub Issue #1](https://github.com/voxmastery/FluctlightDB/issues/1)** or open a Discussion.
