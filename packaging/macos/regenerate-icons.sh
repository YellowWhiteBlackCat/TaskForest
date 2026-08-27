#!/usr/bin/env bash
# Compatibility entry point. The canonical source-to-platform icon workflow
# now lives at packaging/regenerate-icons.sh and also rebuilds Windows ICO.
set -euo pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec bash "$REPO_ROOT/packaging/regenerate-icons.sh"
