#!/usr/bin/env bash
# Install fluctlightdb into a virtual environment (works on PEP 668 / externally-managed Python).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENV="${FLUCTLIGHT_VENV:-${ROOT}/.venv}"

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 not found. On Debian/Ubuntu: sudo apt install python3-full python3-venv" >&2
  exit 1
fi

if [[ ! -d "$VENV" ]]; then
  python3 -m venv "$VENV"
fi

# shellcheck source=/dev/null
source "$VENV/bin/activate"
python -m pip install -U pip
python -m pip install fluctlightdb "$@"

cat <<EOF

Installed fluctlightdb into: $VENV

Activate before use:
  source "$VENV/bin/activate"

Or run without activating:
  "$VENV/bin/python" -c "from fluctlightdb import FluctlightClient; print('ok')"
EOF
