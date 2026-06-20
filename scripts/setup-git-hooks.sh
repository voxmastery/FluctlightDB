#!/usr/bin/env bash
# Enable repo git hooks (strips automated co-author trailers from commits).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
git config core.hooksPath .githooks
echo "Git hooks enabled: $ROOT/.githooks"
