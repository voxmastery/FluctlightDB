# FluctlightDB — Agent Guide

Embedded memory engine for AI agents. Rust core, Python/TS SDKs, MCP server.
Core loop: `experience()` (write episode) → `activate()` (cue-driven recall) → `checkpoint()` / `sleep()` (persist / consolidate).

## Layout

| Path | What |
|------|------|
| `crates/fluctlightdb/` | Rust engine (~15k LOC). Brain-metaphor modules: `brain.rs` (top API), `hippocampus.rs` (episodic store), `index/` (lexical FTS + semantic HNSW hybrid recall), `sleep.rs`/`compact.rs` (consolidation), `serve.rs` (HTTP server), `wal.rs` (durability) |
| `crates/fluctlight-cli/` | `fluctlight` CLI binary |
| `crates/fluctlight-py/` | PyO3 native bindings (`fluctlightdb_native` wheel) |
| `sdks/python/` | Python SDK + MCP server (`fluctlightdb.mcp_server`, 8 tools), project brains, handoffs, doctor |
| `sdks/typescript/` | TS client (single file, HTTP only) |
| `embed-server/` | Python embedding sidecar (has own `.venv`) |
| `benchmarks/` | LoCoMo / LongMemEval / BEIR harnesses; frozen results in `results/*.json` |
| `docs/` | GETTING_STARTED, CLI, DEPLOYMENT, MULTI_AGENT, Manifesto, openapi.yaml, runbooks |
| `scripts/` | backup / restore / failover / drill / bench shell scripts |
| `systemd/` | serve + backup timer units |

## Build & test

```bash
cargo build --release
cargo test --workspace          # all green expected; bench-style suites take minutes
cargo clippy --release --all-targets && cargo fmt --all -- --check   # CI gates
cd sdks/python && python3 -m unittest discover -s tests              # 12 tests, fast
```

## Key invariants & gotchas

- **Recall cap:** `index::hybrid_candidates` clamps caller cap to `MAX_CANDIDATE_CAP` (4096). `DEFAULT_CANDIDATE_CAP` (128) is the default, NOT an upper bound — bench runs use k=150.
- **Provenance ranking:** trusted sources (tool results, files) outrank chat at recall time — don't flatten salience/provenance when touching ranking.
- **Benchmarks are frozen claims:** README numbers (98.1% LoCoMo, 98.0% LongMemEval-S) map to `benchmarks/results/paper-*.json`. Changing recall code requires re-running before touching README.
- **Metric honesty rule:** retrieval recall ≠ LLM-judge QA. Never compare across metric types in docs.
- One store folder per agent; `store_lock.rs` guards concurrent access — don't open one brain from two processes without the lock.
- Test suites `*_bench.rs` are load/scale benches wearing `#[test]` hats; slow by design.

## MCP (for agent integration)

```bash
pip install 'fluctlightdb[mcp,native]'
python -m fluctlightdb.mcp_server   # tools: status, recall, experience, handoff, ...
```
`FLUCTLIGHT_AGENT` env selects agent identity; `fluctlight-project init` sets up a shared repo brain for Cursor/Claude Code/Codex.
