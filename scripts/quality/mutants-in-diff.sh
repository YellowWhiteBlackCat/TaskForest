#!/usr/bin/env bash
# Mutation-test the pure-logic delta of a PR (cargo-mutants --in-diff).
#
# Purpose: verify the tests actually catch breakage, not just that they run
# ("no menu-listing tests"). Scope is the diff, not the workspace: full-workspace
# mutation of 2k+ tests is an extended-tier job, and mutants' own docs say to
# run it on changed code.
#
# Expected scores by code shape (do NOT chase 100%):
#  - decision logic (reducers, source-status ladders, token mapping): 80%+ is
#    attainable; MISSED mutants there are real test gaps.
#  - parsers with boundary-equal mutations (a truncated frame yields None both
#    before and after the mutant): many MISSED mutants are un-distinguishable
#    and should be left alone, not papered over with listing tests.
#
# Options:
#   --packages <a,b>   restrict mutation to these workspace packages
#                      (default: every package the diff touches)
#   --min-score <n>    fail when caught/(missed+caught) falls below n percent
#                      (default: report only)
#   --timeout <sec>    per-cargo-command deadline passed to cargo mutants
#                      (default 300)
#
# Usage: scripts/quality/mutants-in-diff.sh [<base-ref>] [options]
#   Default base is the merge-base with origin/main (fallback HEAD~1).

set -u

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"

export CARGO_BUILD_JOBS="${JOBS:-4}"

base=""
packages=()
min_score=""
mutants_timeout=300

while [[ $# -gt 0 ]]; do
    case "$1" in
    --packages)
        IFS=',' read -r -a packages <<<"$2"
        shift 2
        ;;
    --min-score)
        min_score="$2"
        shift 2
        ;;
    --timeout)
        mutants_timeout="$2"
        shift 2
        ;;
    -*)
        echo "unknown argument '$1'" >&2
        exit 2
        ;;
    *)
        base="$1"
        shift
        ;;
    esac
done

if [[ -z "$base" ]]; then
    if git rev-parse --verify origin/main >/dev/null 2>&1; then
        base="$(git merge-base HEAD origin/main)"
    else
        base="HEAD~1"
    fi
fi

scratch="$(mktemp -d "$repo/.tmp/mutants-XXXXXX")"

cleanup() {
    local rc=$?
    rm -rf "$scratch"
    exit "$rc"
}
trap cleanup EXIT INT TERM

package_args=()
for package in "${packages[@]}"; do
    package_args+=("-p" "$package")
done

# cargo-mutants 27.x takes a diff FILE for --in-diff, not a git ref. Generate
# it here so callers can keep passing a ref/base as before. The scratch copy
# must honor gitignore because repository-local gate artifacts live in `.tmp/`
# and are not mutation inputs.
diff_file="$scratch/diff.patch"
if ! git diff "$base" -- . ':(exclude)Cargo.lock' >"$diff_file"; then
    echo "FAIL mutants-in-diff: git diff $base failed"
    exit 1
fi

echo "mutants-in-diff: base=$base packages=${packages[*]:-<diff-scope>} diff-lines=$(wc -l <"$diff_file")"
output=""
output="$(timeout --kill-after=30s 3600 cargo mutants --in-diff "$diff_file" \
    "${package_args[@]}" --gitignore true --test-tool nextest \
    -t "$mutants_timeout" -o "$scratch/out" 2>&1)"
rc=$?
echo "$output"
# cargo-mutants exit codes: 0 = clean, 2 = run finished with missed mutants
# (the normal "test gap" signal), anything else = tool/run failure.
if [[ $rc -ne 0 && $rc -ne 2 ]]; then
    echo "FAIL mutants-in-diff: cargo mutants exited $rc"
    exit 1
fi

if printf '%s\n' "$output" | grep -q -e "Diff file is empty" -e "No mutants to filter"; then
    echo "PASS mutants-in-diff: no mutant-able changes in the selected packages"
    exit 0
fi

summary="$(printf '%s\n' "$output" | grep -E '[0-9]+ mutants tested in' | tail -n 1)"
if [[ -z "$summary" ]]; then
    echo "FAIL mutants-in-diff: no 'N mutants tested' summary line (unexpected output)"
    exit 1
fi

missed="$(printf '%s\n' "$summary" | sed -n 's/.*: \([0-9]*\) missed.*/\1/p')"
caught="$(printf '%s\n' "$summary" | sed -n 's/.* \([0-9]*\) caught.*/\1/p')"
tested=$((missed + caught))

if [[ -z "$min_score" || "$tested" -eq 0 ]]; then
    echo "mutants-in-diff: $summary (report only)"
    exit 0
fi

score=$((caught * 100 / tested))
if [[ "$score" -lt "$min_score" ]]; then
    echo "FAIL mutants-in-diff: score ${score}% (${caught}/${tested}) below ${min_score}% — $summary"
    exit 1
fi
echo "PASS mutants-in-diff: score ${score}% (${caught}/${tested}) meets ${min_score}% — $summary"
