#!/usr/bin/env bash
# Install every benchmark project at its pinned versions (CI only).
set -euo pipefail
P=${1:-bench/projects}
for d in "$P"/*/; do
  d=${d%/}
  if [ -f "$d/package.json" ]; then
    (cd "$d" && npm install --ignore-scripts --no-audit --no-fund --silent) &
  fi
done
wait
for d in "$P"/*/; do
  d=${d%/}
  if [ -f "$d/requirements.txt" ]; then
    python3 -m venv "$d/.venv" && "$d/.venv/bin/pip" install -q -r "$d/requirements.txt"
  fi
  if [ -f "$d/Cargo.toml" ]; then
    (cd "$d" && cargo generate-lockfile -q && cargo fetch -q)
  fi
done
ls "$P"
