# Fluctlight Swarm Memory

Parallel coding agents are fast, but they do not share a durable account of what was tried, what failed, or which result was actually verified. They can receive the same context, repeat the same dead end, and turn a worker's confident claim into team knowledge without evidence.

Fluctlight Swarm Memory is a Codex plugin backed by FluctlightDB. It gives a parallel Codex run:

- shared verified truth and mandatory failure warnings;
- disjoint episodic memories so workers explore different strategies;
- worker and worktree identity binding;
- citations restricted to memories actually exposed to that worker;
- evidence-gated outcomes—workers report attempts, but only an admin/verifier can accept them;
- targeted learning: success or reproduced failure updates only the memories that were cited;
- WAL and v4 checkpoint recovery across coordinator restarts.

## One-command demo

Prerequisites: Rust/Cargo and Python 3.9+.

[Watch the 54-second Remotion terminal demo](docs/demo/fluctlight-swarm-memory-demo.mp4), inspect its [reproducible source](demo/remotion), or run the same verified flow yourself:

```bash
python3 scripts/demo_codex_swarm.py
```

The demo launches an authenticated local coordinator, assigns two non-overlapping memory bundles, proves that a worker cannot cite another worker's memory, proves that a worker cannot verify its own result, accepts trusted evidence, finishes the run, restarts the coordinator, and confirms the completed state survived.

Expected final line:

```text
PASS: durable, diverse, evidence-gated swarm memory survived restart
```

## How it connects to Codex

The plugin is in [`plugins/fluctlight-swarm`](plugins/fluctlight-swarm). It packages:

- an MCP server with five swarm lifecycle tools;
- `SubagentStart` and `SubagentStop` hooks;
- a Skill that requires the root agent to declare the full roster before spawning workers.

Codex calls `fluctlight_swarm_begin` once. Each `SubagentStart` hook claims one unique slot and injects only that slot's bounded memory bundle. Each `SubagentStop` hook records a pending attempt tied to its Git tree. Trusted repository tests provide the evidence; a worker cannot self-certify.

The current prototype intentionally keeps final evidence approval with the root/verifier. This is a safety boundary, not an autonomous-success claim.

## What Codex contributed

Codex was used as the engineering environment, not merely as a text generator. Parallel analysis agents audited FluctlightDB's Rust persistence model and the open-source Codex hook/plugin surfaces. Codex then designed the transaction model, wrote the Rust coordinator and tests, built the MCP plugin, found an MCP 2.0 compatibility issue during a live smoke test, added a regression test, and reran the complete verification suite.

## Verification

- complete `cargo test -p fluctlightdb` suite, including the 10,000-memory load smoke test;
- HTTP lifecycle and role-enforcement integration tests;
- WAL replay and v4 checkpoint round trips;
- Python client and MCP 2.0 tool-registration tests;
- Codex plugin and Skill validators;
- the end-to-end demo above, including restart recovery.

Design and source audit: [`docs/superpowers/specs/2026-08-15-codex-native-swarm-memory-design.md`](docs/superpowers/specs/2026-08-15-codex-native-swarm-memory-design.md) · [`docs/CODEX_SWARM_SOURCE_AUDIT.md`](docs/CODEX_SWARM_SOURCE_AUDIT.md)
