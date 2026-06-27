#!/usr/bin/env bash
# Sync paper artifacts for Hugging Face + private VPS viewer.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

bash scripts/sync-paper-public.sh

echo "Synced papers/public (HF Space + optional local mirror)"
echo ""
echo "Hugging Face (HF_ORG=${HF_ORG:-Voxiesz}):"
echo "  bash scripts/publish-paper-huggingface.sh"
echo ""
echo "Private VPS viewer:"
echo "  ./papers/site/install-search-site.sh"
echo ""
echo "Venue plan: docs/RESEARCH_VENUES.md"
