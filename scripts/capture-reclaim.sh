#!/usr/bin/env bash
# Reap only stale UUID-scoped capture runs; never inspect or signal host GUI
# processes by name.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
exec timeout --kill-after=5s 30s python3 "$REPO/scripts/capture_supervisor.py" \
  --repo-root "$REPO" --reclaim
