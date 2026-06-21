#!/usr/bin/env bash
# Set Git identity for FluctlightDB contributions (links commits to @voxmastery on GitHub).
set -euo pipefail

GIT_NAME="${FLUCTLIGHT_GIT_NAME:-Voxmastery}"
GIT_EMAIL="${FLUCTLIGHT_GIT_EMAIL:-roppashreeganesh@gmail.com}"

git config user.name "$GIT_NAME"
git config user.email "$GIT_EMAIL"

if [[ "${FLUCTLIGHT_GIT_GLOBAL:-1}" == "1" ]]; then
  git config --global user.name "$GIT_NAME"
  git config --global user.email "$GIT_EMAIL"
  echo "Global git identity set."
fi

echo "Git identity:"
echo "  name:  $(git config user.name)"
echo "  email: $(git config user.email)"
echo ""
echo "Ensure $GIT_EMAIL is verified at https://github.com/settings/emails"
