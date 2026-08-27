#!/usr/bin/env bash
# Stable entry point for the Iced evidence workflow.
#
# The implementation lives in capture-iced-matrix.sh so the public command
# mirrors GPUI's capture-niri.sh while making the one-build/one-Niri/sequential
# scenario contract explicit.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
exec bash "$REPO/scripts/capture-iced-matrix.sh" "$@"
