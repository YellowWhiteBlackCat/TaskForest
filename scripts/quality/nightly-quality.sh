#!/usr/bin/env bash
# Nightly quality pass: the expensive checks that do NOT belong on every PR.
#
# Stages (each independently skippable, all bounded by `timeout`):
#   1. Miri      — the three Linux audited unsafe boundary crates
#                  (ADR-022/024/025); the Windows root is compile/clippy-tested.
#                  Caught a real CMSG-alignment UB in fd-bridge (2026-08-07).
#   2. Fuzz      — libFuzzer over the five-tuple frame parser (untrusted wire
#                  bytes); 2M runs are ~1s of corpus-guided coverage.
#   3. Mutants   — mutation score on the diff since origin/main (or HEAD~1);
#                  decision logic should reach 80%+, parser boundary-equal
#                  mutants are expected to miss and are not failures.
#   4. Bloat     — release binary size trend (5% single-release growth review).
#
# Usage:
#   scripts/quality/nightly-quality.sh [--miri|--fuzz|--mutants|--bloat]
#   Run with no args to run all stages.

set -u

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"

stage="${1:-all}"
failures=0

run_stage() {
    local name="$1"
    shift
    echo "=== nightly: $name ==="
    if ! timeout "${NIGHTLY_TIMEOUT:-900}" "$@"; then
        echo "FAIL nightly: $name"
        failures=$((failures + 1))
    fi
}

if [[ "$stage" == "all" || "$stage" == "--miri" ]]; then
    run_stage miri scripts/quality/miri-boundaries.sh
fi
if [[ "$stage" == "all" || "$stage" == "--fuzz" ]]; then
    run_stage fuzz bash -c \
        'cd crates/taskmanager-afpacket && cargo +nightly fuzz run five_tuple -- -runs=2000000 -timeout=5'
fi
if [[ "$stage" == "all" || "$stage" == "--mutants" ]]; then
    run_stage mutants scripts/quality/mutants-in-diff.sh
fi
if [[ "$stage" == "all" || "$stage" == "--bloat" ]]; then
    run_stage bloat bash -c \
        'cargo bloat --release --bin taskmanager 2>/dev/null | head -20'
fi

if [[ $failures -gt 0 ]]; then
    echo "nightly-quality: $failures stage(s) failed"
    exit 1
fi
echo "nightly-quality: clean"
