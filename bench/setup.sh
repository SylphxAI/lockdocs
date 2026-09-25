#!/usr/bin/env bash
# Install every benchmark project at its pinned versions (CI only).
# npm projects: a per-project .npmrc (e.g. legacy-peer-deps=true) is honoured
# by npm itself. Python projects: each has its own requirements.txt and venv.
set -euo pipefail
P=${1:-bench/projects}
pids=()
for d in "$P"/*/; do
  d=${d%/}
  if [ -f "$d/package.json" ]; then
    (cd "$d" && npm install --ignore-scripts --no-audit --no-fund --silent) &
    pids+=($!)
  fi
done
for pid in "${pids[@]}"; do wait "$pid"; done
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
