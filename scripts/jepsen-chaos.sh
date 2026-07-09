#!/usr/bin/env bash
# Jepsen-style chaos suite (embedded brain — not a distributed Raft cluster).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "==> Building chaos worker binary"
cargo build -q -p fluctlightdb --bin fluctlight-chaos-worker --release

echo "==> crash_recovery integration tests"
cargo test -q -p fluctlightdb --test crash_recovery --release

echo "==> chaos_jepsen harness"
cargo test -q -p fluctlightdb --test chaos_jepsen --release

echo "PASS — chaos / crash recovery suite"
