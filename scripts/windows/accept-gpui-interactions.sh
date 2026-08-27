#!/usr/bin/env bash
# Windows-only entry for the headless GPUI interaction acceptance gate.
#
# The behavior matrix is toolkit- and host-neutral, but its execution lease and
# evidence must not share the Linux route. Pixel capture remains a separate
# Windows.Graphics.Capture workflow.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"

case "$(uname -s)" in
MINGW* | MSYS* | CYGWIN*) ;;
*)
    printf 'this GPUI interaction entry is Windows-only (uname: %s)\n' "$(uname -s)" >&2
    exit 2
    ;;
esac

export GPUI_INTERACTION_SCOPE=windows
export GPUI_INTERACTION_EVIDENCE_ROOT="$repo/target/windows-gpui-interaction-evidence"
export GPUI_INTERACTION_WORKDIR_TASK=windows-gpui-interactions
export GPUI_INTERACTION_COMMAND='bash scripts/windows/accept-gpui-interactions.sh'

exec bash scripts/accept-gpui-interactions.sh
