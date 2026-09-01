#!/usr/bin/env bash
# Run the fail-closed GPUI interaction acceptance gate.
#
# This is headless acceptance: GPUI's TestAppContext/VisualTestContext drives the
# real event dispatch path without requiring a Wayland compositor. Pixel proof is
# intentionally a separate capture-niri.sh gate.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"

scope="${GPUI_INTERACTION_SCOPE:-linux}"
evidence_root="${GPUI_INTERACTION_EVIDENCE_ROOT:-$REPO/target/gpui-interaction-evidence}"
workdir_task="${GPUI_INTERACTION_WORKDIR_TASK:-gpui-interactions}"
case "$scope" in
windows)
    case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*) ;;
    *)
        printf 'Windows GPUI interaction scope requires Git Bash on Windows (uname: %s)\n' \
            "$(uname -s)" >&2
        exit 2
        ;;
    esac
    ;;
linux)
    case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*)
        printf 'Linux GPUI interaction scope is unavailable on Windows; use scripts/windows/accept-gpui-interactions.sh\n' >&2
        exit 2
        ;;
    esac
    ;;
*)
    printf 'unknown GPUI interaction scope: %s\n' "$scope" >&2
    exit 2
    ;;
esac

for command in cargo git rustc tee timeout; do
    if ! command -v "$command" >/dev/null 2>&1; then
        printf 'required acceptance command is unavailable: %s\n' "$command" >&2
        exit 2
    fi
done
if ! timeout 5s python3 --version >/dev/null 2>&1; then
    printf 'required Python 3 interpreter is unavailable\n' >&2
    exit 2
fi

eval "$(scripts/agent-workdir.sh enter "$workdir_task")"

RUN_STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
GIT_HEAD="$(git rev-parse --short=12 HEAD 2>/dev/null || printf 'no-git')"
WORKTREE_STATE=clean
if [ -n "$(git status --porcelain)" ]; then
    WORKTREE_STATE=dirty
fi
RUN_ID="${RUN_STAMP}_${GIT_HEAD}_${WORKTREE_STATE}_$$"
RUN_DIR="$evidence_root/$RUN_ID"
mkdir -p "$RUN_DIR"

printf 'run_id=%s\n' "$RUN_ID" >"$RUN_DIR/metadata.txt"
printf 'scope=%s\n' "$scope" >>"$RUN_DIR/metadata.txt"
printf 'git_head=%s\n' "$GIT_HEAD" >>"$RUN_DIR/metadata.txt"
printf 'worktree=%s\n' "$WORKTREE_STATE" >>"$RUN_DIR/metadata.txt"
printf 'rust=%s\n' "$(rustc -V)" >>"$RUN_DIR/metadata.txt"
printf 'evidence_root=%s\n' "$evidence_root" >>"$RUN_DIR/metadata.txt"
printf 'command=%s\n' "${GPUI_INTERACTION_COMMAND:-bash scripts/accept-gpui-interactions.sh}" \
    >>"$RUN_DIR/metadata.txt"

timeout 30s python3 scripts/validate_gpui_interaction_matrix.py --self-test

timeout --kill-after=10s 20m cargo nextest list --locked --profile ci \
    -p taskmanager-gpui --test gui --features test-support \
    --message-format json >"$RUN_DIR/gui-list.json"
# The interaction matrix's `lib` rows and the `gui` integration binary both
# belong to taskmanager-gpui after ADR-051. The precise package
# scope also keeps this gate runnable on every platform: workspace-wide lib
# builds would pull the single-platform adapters (platform-linux/macos do not
# compile on Windows; platform-linux needs the Linux artifact set).
timeout --kill-after=10s 20m cargo nextest list --locked --profile ci \
    -p taskmanager-gpui --lib \
    --message-format json >"$RUN_DIR/lib-list.json"

timeout 30s python3 scripts/validate_gpui_interaction_matrix.py \
    --matrix scripts/gpui_interaction_matrix.tsv \
    --requirements scripts/interaction_requirements.tsv \
    --capture-matrix scripts/capture_scenarios.tsv \
    --gui-list "$RUN_DIR/gui-list.json" \
    --lib-list "$RUN_DIR/lib-list.json" \
    --receipt "$RUN_DIR/matrix-validation.json"

run_nextest() {
    local output="$1"
    shift
    NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1 \
        timeout --kill-after=10s 30m cargo nextest run --locked --profile ci \
        --message-format libtest-json-plus --cargo-quiet --no-fail-fast -j 4 "$@" \
        >"$output" 2>&1
}

run_nextest "$RUN_DIR/gui-run.log" -p taskmanager-gpui --test gui --features test-support
run_nextest "$RUN_DIR/lib-run.log" -p taskmanager-gpui --lib

timeout 30s python3 scripts/validate_gpui_interaction_matrix.py \
    --matrix scripts/gpui_interaction_matrix.tsv \
    --requirements scripts/interaction_requirements.tsv \
    --capture-matrix scripts/capture_scenarios.tsv \
    --gui-list "$RUN_DIR/gui-list.json" \
    --lib-list "$RUN_DIR/lib-list.json" \
    --run-log "$RUN_DIR/gui-run.log" \
    --run-log "$RUN_DIR/lib-run.log" \
    --receipt "$RUN_DIR/interaction-validation.json"

printf 'GPUI interaction acceptance: PASS\n'
printf 'receipt=%s\n' "$RUN_DIR/interaction-validation.json"
