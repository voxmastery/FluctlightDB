#!/usr/bin/env bash
# Start embed sidecar with retrieval-tuned model (multi-qa-mpnet) on port 8794.
# Point benchmarks at it: export FLUCTLIGHT_EMBED_URL=http://127.0.0.1:8794
set -euo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
PORT="${FLUCTLIGHT_EMBED_PORT:-8794}"
export FLUCTLIGHT_EMBED_MODEL="${FLUCTLIGHT_EMBED_MODEL:-sentence-transformers/multi-qa-mpnet-base-dot-v1}"
export FLUCTLIGHT_EMBED_HOST=127.0.0.1
export FLUCTLIGHT_EMBED_PORT="$PORT"
cd "${REPO}/embed-server"
if [[ ! -d .venv ]]; then
  python3 -m venv .venv
  .venv/bin/pip install -q fastapi uvicorn sentence-transformers
fi
echo "Starting embed server model=$FLUCTLIGHT_EMBED_MODEL port=$PORT"
exec .venv/bin/python -m uvicorn main:app --host 127.0.0.1 --port "$PORT"
