#!/usr/bin/env bash
# Unified cross-frontend interaction acceptance driver (S5 step 1).
#
# Thin entry point: the driver logic is committed as
# scripts/parity/accept_frontend_interactions.py so the segment and aggregation
# work stays testable without a shell.  This wrapper owns one thing only: the
# external deadline.  Every external command this repository launches from a
# shell is wrapped in `timeout --kill-after=...` (automation-safety discipline),
# and the Python driver wraps each child it spawns the same way.
#
# Usage:
#   bash scripts/parity/accept-frontend-interactions.sh [FRONTEND ...] [options]
#   bash scripts/parity/accept-frontend-interactions.sh --help
#   bash scripts/parity/accept-frontend-interactions.sh --self-test
#
# Examples:
#   # Full four-frontend acceptance (the S5 gate shape; not wired into
#   # local-gates.sh until the owner opens the S5 window):
#   bash scripts/parity/accept-frontend-interactions.sh
#
#   # No-GUI end-to-end rehearsal from pre-existing discovery artifacts:
#   bash scripts/parity/accept-frontend-interactions.sh --from-existing \
#       --discovery gpui=... --discovery iced=... --discovery tui=... --discovery bevy=...
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO"

for command_name in timeout python3; do
    if ! command -v "$command_name" >/dev/null 2>&1; then
        printf 'required driver command is unavailable: %s\n' "$command_name" >&2
        exit 2
    fi
done

# The driver internally bounds every child; this deadline bounds the whole run.
driver_deadline="${ACCEPT_FRONTEND_INTERACTIONS_DEADLINE:-240m}"

exec timeout --kill-after=30s "$driver_deadline" python3 scripts/parity/accept_frontend_interactions.py "$@"
