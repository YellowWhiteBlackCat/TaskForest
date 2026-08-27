#!/usr/bin/env bash
# bench-gate.sh — run the zero-dependency throughput benches and compare
# each measurement against the committed trend.
#
# The bench binary prints one `RESULT<TAB><name><TAB><ns>` line per
# measurement (contract in crates/taskmanager-platform-linux/benches/
# throughput.rs). This gate runs it under a hard deadline, then fails any
# measurement that regressed more than 100% against the newest recorded row
# for that name in docs/quality/bench-trend.tsv — a loose tolerance for a
# shared, loaded machine; the point is catching 10x accidental regressions,
# not nano-noise. First run seeds the trend file.
#
# Usage: scripts/quality/bench-gate.sh

set -u

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"

export CARGO_BUILD_JOBS="${JOBS:-4}"
trend_path="$repo/docs/quality/bench-trend.tsv"
scratch="$(mktemp -d "$repo/.tmp/bench-gate-XXXXXX")"
failures=0

cleanup() {
    local rc=$?
    rm -rf "$scratch"
    exit "$rc"
}
trap cleanup EXIT INT TERM

echo "=== bench-gate ==="
# The hot parsers are re-exported behind `test-support`; enable it for the
# bench binary only (a dev-time gate, never a shipped artifact).
if ! timeout --kill-after=30s 900 cargo bench -p taskmanager-platform-linux --bench throughput --features test-support >"$scratch/out.txt" 2>&1; then
    tail -20 "$scratch/out.txt"
    echo "FAIL bench-gate: bench binary did not run to completion"
    exit 1
fi

grep '^RESULT' "$scratch/out.txt" >"$scratch/results.tsv" || {
    echo "FAIL bench-gate: no RESULT lines in bench output"
    exit 1
}

while IFS=$'\t' read -r _ name ns; do
    previous=""
    if [[ -f "$trend_path" ]]; then
        previous="$(grep -P "\t${name}\t" "$trend_path" | tail -n 1 | cut -f 3)"
    fi
    if [[ -z "$previous" ]]; then
        echo "PASS $name: seeded ${ns}ns (no history)"
    elif ((ns > previous * 2)); then
        echo "FAIL $name: ${ns}ns > 2x previous ${previous}ns"
        failures=$((failures + 1))
    else
        echo "PASS $name: ${ns}ns vs previous ${previous}ns"
    fi
    printf '%s\t%s\t%s\n' "$(date +%F)" "$name" "$ns" >>"$trend_path"
done <"$scratch/results.tsv"

if [[ $failures -gt 0 ]]; then
    echo "bench-gate: $failures measurement(s) regressed"
    exit 1
fi
echo "bench-gate: clean"
