#!/usr/bin/env bash
# Resolve fluctlight binary: env > PATH > local release build.
if [[ -n "${FLUCTLIGHT_BIN:-}" ]]; then
  echo "$FLUCTLIGHT_BIN"
elif command -v fluctlight >/dev/null 2>&1; then
  command -v fluctlight
else
  REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  echo "$REPO_ROOT/target/release/fluctlight"
fi
