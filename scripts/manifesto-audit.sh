#!/usr/bin/env bash
# Run Manifesto.md audit checklist (pass/fail per principle).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
echo "Running manifesto audit (release tests)..."
cargo test --release -p fluctlightdb --test manifesto_audit -- --nocapture
echo ""
echo "Running production bench (ledger truth + separation gate)..."
cargo test --release -p fluctlightdb --test prod_bench -- --nocapture
