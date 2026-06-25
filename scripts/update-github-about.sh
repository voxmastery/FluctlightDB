#!/usr/bin/env bash
# Update GitHub repo About (description, homepage, topics). Requires repo admin token.
set -euo pipefail

: "${GITHUB_TOKEN:?Set GITHUB_TOKEN (repo scope)}"

OWNER="${GITHUB_OWNER:-voxmastery}"
REPO="${GITHUB_REPO:-FluctlightDB}"

DESCRIPTION="Embedded memory engine for AI agents: third data model (not SQL/vectors). experience()+activate(), provenance, benchmarks. Goal: SQLite for agent memory."
HOMEPAGE="https://search.ambugo.help/paper/"
TOPICS='agent-memory ai-agents brain-native database docker episodic-memory llm memory pypi python rust'

auth=(-H "Authorization: Bearer ${GITHUB_TOKEN}" -H "Accept: application/vnd.github+json")

echo "Updating description and homepage…"
curl -sS -X PATCH "https://api.github.com/repos/${OWNER}/${REPO}" \
  "${auth[@]}" \
  -d "$(python3 - <<PY
import json
print(json.dumps({
    "description": """${DESCRIPTION}""",
    "homepage": """${HOMEPAGE}""",
}))
PY
)" | python3 -c "import sys,json; r=json.load(sys.stdin); print('description:', r.get('description','?')[:80])"

echo "Updating topics…"
curl -sS -X PUT "https://api.github.com/repos/${OWNER}/${REPO}/topics" \
  -H "Authorization: Bearer ${GITHUB_TOKEN}" \
  -H "Accept: application/vnd.github+json" \
  -d "$(python3 - <<PY
import json
print(json.dumps({"names": """${TOPICS}""".split()}))
PY
)" | python3 -c "import sys,json; r=json.load(sys.stdin); print('topics:', ', '.join(r.get('names',[])))"

echo "Done. Check: https://github.com/${OWNER}/${REPO}"
