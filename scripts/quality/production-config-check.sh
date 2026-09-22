#!/usr/bin/env bash
# Production-config gate: compile each shipped frontend product WITHOUT the
# dev-only `test-support` feature and under `-D warnings`.
#
# Why this exists: the workspace clippy stage (and every nextest layer) enables
# `test-support`, so code compiled only when that feature is OFF is never
# compiled by a local gate. A warning there — e.g. a binding used solely by a
# `#[cfg(any(test, feature = "test-support"))]` debug selector, which is what
# broke macOS CI on 2026-09-22 — passes every local stage and fails the native
# portability jobs. This stage mirrors those product-config commands locally:
#
#   gpui  cargo check --bin taskforest-g              (default features)
#         cargo check --lib --no-default-features     (reduced product fallback)
#   iced  cargo check --bin taskforest-i              (default features)
#   tui   cargo check --bins                          (taskforest-t + taskmanager-tui)
#   bevy  cargo check --bin taskforest-b              (default features)
#
# `check` (not `build`) keeps the stage inside the check-artifact cache and
# still runs every rustc lint; `-D warnings` is the same policy as CI's
# workflow-level RUSTFLAGS. Each command is bounded by its own external
# `timeout` where the host provides a GNU one (see the deadline-selection
# block below).
#
# Usage:
#   scripts/quality/production-config-check.sh [gpui|iced|tui|bevy ...]
# No argument selects all four product crates. The standard gate passes the
# scope's frontend set so a scoped line only compiles its own product.
#
# Environment:
#   PRODUCTION_CONFIG_TIMEOUT=<sec>  per-command deadline (default 900)
#   TM_CARGO_LOCK                    set-but-empty replicates the gate's
#                                    unlocked dev-phase fallback
#   CARGO_BUILD_JOBS                 parallelism (default 4, capped by -j 4)
#
# Portability: the macOS runner image ships no GNU coreutils, so neither
# `timeout` nor `gtimeout` exists there, and Git Bash can resolve Windows'
# System32 `timeout.exe`, which rejects GNU flags. The selection block below
# therefore probes `--kill-after` behavior instead of trusting `command -v`
# alone; with no usable tool the stage still runs every command and prints
# "no external timeout available", leaving the caller's budget (the `timeout`
# wrapper in ci.yml, `run_stage`'s deadline in local-gates.sh, the portability
# job's `timeout-minutes` on macOS) as the only deadline. On such a host
# PRODUCTION_CONFIG_TIMEOUT is inert.

set -u

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}"
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"

# Same warning policy as CI (portability.yml / packaging.yml env) and as the
# parent gate: append `-D warnings` unless it is already there.
rustflags="${RUSTFLAGS:-}"
if [[ "$rustflags" != *"-D warnings"* ]]; then
    rustflags="${rustflags:+$rustflags }-D warnings"
fi
export RUSTFLAGS="$rustflags"

# Gate-wide lock policy: unset = --locked; set-but-empty = the documented
# dev-phase unlocked fallback the parent selected after its settle window.
lock_flag="${TM_CARGO_LOCK---locked}"
lock_args=()
[[ -n "$lock_flag" ]] && lock_args=("$lock_flag")

check_timeout="${PRODUCTION_CONFIG_TIMEOUT:-900}"

# Portable external deadline (see the portability note above): prefer GNU
# `timeout`, accept Homebrew's `gtimeout` (same GNU coreutils flags), and
# otherwise run unbounded with a printed notice. The probe runs `true` under a
# short deadline so a shadowed non-GNU `timeout` is rejected by behavior, not
# by name.
timeout_bin=""
if command -v timeout >/dev/null 2>&1 && timeout --kill-after=1s 5s true >/dev/null 2>&1; then
    timeout_bin="timeout"
elif command -v gtimeout >/dev/null 2>&1 && gtimeout --kill-after=1s 5s true >/dev/null 2>&1; then
    timeout_bin="gtimeout"
fi

frontends="${*:-gpui iced tui bevy}"
echo "production-config: frontends=$frontends"
echo "production-config: RUSTFLAGS=$RUSTFLAGS"
if [[ -n "$timeout_bin" ]]; then
    echo "production-config: lock=${lock_flag:-unlocked} timeout=${check_timeout}s per command (via $timeout_bin)"
else
    echo "production-config: lock=${lock_flag:-unlocked} no external timeout available (timeout/gtimeout unusable); commands run unbounded" >&2
fi

failed_labels=()

run_check() {
    local label="$1"
    shift
    echo ""
    echo "--- $label"
    echo "+ $*"
    local rc=0
    case "$timeout_bin" in
    timeout)
        timeout --kill-after=30s "$check_timeout" "$@" || rc=$?
        ;;
    gtimeout)
        gtimeout --kill-after=30s "$check_timeout" "$@" || rc=$?
        ;;
    *)
        "$@" || rc=$?
        ;;
    esac
    if [[ $rc -eq 0 ]]; then
        echo "PASS $label"
    else
        echo "FAIL $label (rc=$rc)" >&2
        failed_labels+=("$label")
    fi
    return 0
}

for frontend in $frontends; do
    case "$frontend" in
    gpui)
        run_check "gpui: taskforest-g (default features)" \
            cargo check "${lock_args[@]}" -j 4 -p taskmanager-gpui --bin taskforest-g
        run_check "gpui: lib (no default features)" \
            cargo check "${lock_args[@]}" -j 4 -p taskmanager-gpui --lib --no-default-features
        ;;
    iced)
        run_check "iced: taskforest-i (default features)" \
            cargo check "${lock_args[@]}" -j 4 -p taskmanager-iced --bin taskforest-i
        ;;
    tui)
        run_check "tui: taskforest-t + taskmanager-tui (default features)" \
            cargo check "${lock_args[@]}" -j 4 -p taskmanager-tui --bins
        ;;
    bevy)
        run_check "bevy: taskforest-b (default features)" \
            cargo check "${lock_args[@]}" -j 4 -p taskmanager-bevy-ui --bin taskforest-b
        ;;
    *)
        echo "production-config-check: unknown frontend '$frontend' (gpui|iced|tui|bevy)" >&2
        exit 2
        ;;
    esac
done

if [[ ${#failed_labels[@]} -gt 0 ]]; then
    echo ""
    echo "production-config-check: FAIL (${#failed_labels[@]} command(s))" >&2
    for label in "${failed_labels[@]}"; do
        echo "  failed: $label" >&2
    done
    exit 1
fi

echo ""
echo "production-config-check: PASS"
