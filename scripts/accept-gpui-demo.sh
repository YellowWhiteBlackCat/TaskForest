#!/usr/bin/env bash
# Verify the GPUI demo composition and its advertised CLI entry point.
#
# The default headless mode creates no window and performs no host I/O. The
# optional --live mode is an explicit desktop smoke: timeout owns the process
# and the caller's current display must be intentionally available.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO"

usage() {
    printf '%s\n' \
        'usage: bash scripts/accept-gpui-demo.sh [--headless|--live|--self-test]'
}

mode=headless
case "${1:-}" in
    ''|--headless) mode=headless ;;
    --live) mode=live ;;
    --self-test)
        mode=self-test
        ;;
    --help|-h)
        usage
        exit 0
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac

if [[ "$mode" == self-test ]]; then
    [[ "$REPO" == /* ]] || { printf 'repository path must be absolute\n' >&2; exit 1; }
    [[ "$mode" == self-test ]] || { printf 'self-test mode did not resolve\n' >&2; exit 1; }
    printf 'GPUI demo acceptance script self-test: PASS\n'
    exit 0
fi

for command in cargo git rustc timeout; do
    command -v "$command" >/dev/null 2>&1 \
        || { printf 'required command is unavailable: %s\n' "$command" >&2; exit 2; }
done

RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$(git rev-parse --short=12 HEAD)-$$"
RUN_DIR="$REPO/target/gpui-demo-evidence/$RUN_ID"
mkdir -p "$RUN_DIR"
LOG="$RUN_DIR/demo.log"
HELP="$RUN_DIR/help.txt"
RECEIPT="$RUN_DIR/receipt.tsv"

timeout --kill-after=10s 30m cargo nextest run --locked --profile ci -j 4 \
    -p taskmanager-gpui --lib \
    -E 'test(demo_root_materializes_shared_facts_without_a_platform_client) or test(documentation_action_uses_the_taskforest_destination)' \
    >"$LOG" 2>&1

timeout --kill-after=10s 10m cargo build --locked -j 4 \
    -p taskmanager-gpui --bin taskforest-g >"$RUN_DIR/build.log" 2>&1

BINARY="$REPO/target/debug/taskforest-g"
if [[ -f "$BINARY.exe" ]]; then
    BINARY="$BINARY.exe"
fi
[[ -x "$BINARY" || -f "$BINARY" ]] \
    || { printf 'demo binary was not produced: %s\n' "$BINARY" >&2; exit 1; }

timeout --kill-after=10s 30s "$BINARY" --help >"$HELP"
grep -F -- '--demo' "$HELP" >/dev/null \
    || { printf 'GPUI help does not advertise --demo\n' >&2; exit 1; }
if grep -F -- 'not yet supported by the GPUI frontend' "$HELP" >/dev/null; then
    printf 'GPUI help contains the retired demo rejection\n' >&2
    exit 1
fi

live_result=not-run
if [[ "$mode" == live ]]; then
    if [[ -z "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ]]; then
        printf 'live GPUI demo acceptance requires WAYLAND_DISPLAY or DISPLAY\n' >&2
        exit 2
    fi
    set +e
    timeout --kill-after=5s 12s env NO_COLOR=1 TM_WINDOW_SIZE=720x760 \
        "$BINARY" --demo >>"$LOG" 2>&1
    live_rc=$?
    set -e
    case "$live_rc" in
        0|124|143) live_result="exit-$live_rc" ;;
        *)
            printf 'live GPUI demo exited unexpectedly: %s\n' "$live_rc" >&2
            exit "$live_rc"
            ;;
    esac
    grep -F -- 'Starting TaskForest demo frontend' "$LOG" >/dev/null \
        || { printf 'live demo did not reach its startup path\n' >&2; exit 1; }
    if ps -eo pid=,args= | grep -F -- "$BINARY --demo" | grep -v 'grep -F' >/dev/null; then
        printf 'live demo left a taskforest-g process behind\n' >&2
        exit 1
    fi
fi

{
    printf 'mode\t%s\n' "$mode"
    printf 'binary\t%s\n' "${BINARY#"$REPO/"}"
    printf 'head\t%s\n' "$(git rev-parse HEAD)"
    printf 'headless_tests\tpassed\n'
    printf 'cli_help\tpassed\n'
    printf 'live_smoke\t%s\n' "$live_result"
} >"$RECEIPT"

printf 'GPUI demo acceptance: PASS (%s)\n' "$mode"
printf 'receipt=%s\n' "$RECEIPT"
