#!/usr/bin/env bash
# Fourth frontend's fail-closed headless interaction gate.
#
# The unified matrix names behavior tests, not source symbols. Discovery must
# find every named test in the actual Bevy lib target, then the complete lib
# target runs under the locked workspace. Wayland pixels are a separate gate
# owned by capture-bevy.sh and never inferred from this pass.
#
# D6: the retired per-frontend compatibility view is gone; the unified matrix is
# the single declaration authority and this gate projects its `bevy` rows into
# the run's evidence directory.
set -euo pipefail
export LC_ALL=C

REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"
UNIFIED_MATRIX="$REPO/scripts/parity/cross_frontend_matrix.tsv"
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
OUT="$REPO/target/bevy-interaction-evidence/$RUN_ID"
mkdir -p "$OUT"

die() {
    printf 'Bevy interaction gate: FAIL: %s\n' "$1" >&2
    exit 1
}

# Project the unified matrix's `bevy` rows to the columns this gate consumes.
# The projection is generated per run, never committed; `paths` tokens are
# copied verbatim (their vocabulary authority is the Rust `ContractTag`).
project_matrix() {
    local output="$1"
    [ -s "$UNIFIED_MATRIX" ] \
        || die "unified interaction matrix is empty or missing: $UNIFIED_MATRIX"
    {
        printf 'case_id\ttarget\ttest_name\tpaths\n'
        awk -F'\t' -v OFS='\t' '
            $0 ~ /^[[:space:]]*#/ { next }
            $1 == "subject_kind" { next }
            $3 == "bevy" { print $2, $5, $6, $7 }
        ' "$UNIFIED_MATRIX"
    } >"$output"
}

MATRIX="$OUT/matrix.tsv"
project_matrix "$MATRIX"

validate_matrix() {
    [ -s "$MATRIX" ] || die "missing or empty matrix: $MATRIX"
    [ "$(head -n 1 "$MATRIX")" = $'case_id\ttarget\ttest_name\tpaths' ] \
        || die "unexpected matrix header"
    # The resolver owns the authoritative row schema, the `(frontend, case_id)`
    # key and the target vocabulary; this gate keeps a local shape check.  The
    # unified matrix also lets two cases share one anchor test (for example the
    # icon-stamp and tofu-law cases both drive the spawned-icon scene), so the
    # retired per-frontend view's duplicate test-name rule is deliberately not
    # carried over.
    local case_id target test_name paths count=0
    local -A cases=()
    while IFS=$'\t' read -r case_id target test_name paths; do
        [ -n "${case_id:-}" ] || die "empty case id"
        [ -z "${cases[$case_id]+set}" ] || die "duplicate case id: $case_id"
        [ "$target" = lib ] || die "$case_id: target must be lib"
        [ -n "$test_name" ] || die "$case_id: empty test name"
        [ -n "$paths" ] || die "$case_id: empty behavior path"
        cases[$case_id]=1
        count=$((count + 1))
    done < <(tail -n +2 "$MATRIX")
    [ "$count" -gt 0 ] || die "matrix has no behavior cases"
    MATRIX_COUNT="$count"
}

if [[ "${1:-}" == --self-test ]]; then
    validate_matrix
    printf 'Bevy interaction gate self-test: PASS (%s cases)\n' "$MATRIX_COUNT"
    exit 0
fi

validate_matrix
# Lock policy follows the caller: unset TM_CARGO_LOCK keeps repo law
# (--locked); local-gates' dev-phase fallback exports it empty to run
# unlocked while a sibling line holds the shared lock mid-write.
LOCK_FLAG="${TM_CARGO_LOCK---locked}"
git status --short >"$OUT/git-status.txt"
git rev-parse HEAD >"$OUT/git-head.txt"
rustc -V >"$OUT/rust.txt"
timeout 60s cargo nextest list "$LOCK_FLAG" -p taskmanager-bevy-ui --lib \
    >"$OUT/discovery.txt"
sed 's/^taskmanager-bevy-ui //' "$OUT/discovery.txt" >"$OUT/discovery-names.txt"

missing=0
while IFS=$'\t' read -r case_id target test_name paths; do
    if ! grep -Fxq "$test_name" "$OUT/discovery-names.txt"; then
        printf '%s\t%s\n' "$case_id" "$test_name" >>"$OUT/missing-tests.tsv"
        missing=1
    fi
done < <(tail -n +2 "$MATRIX")
[ "$missing" -eq 0 ] || die "matrix contains an undiscovered test (see $OUT/missing-tests.tsv)"

set +e
timeout --kill-after=10s 20m cargo nextest run "$LOCK_FLAG" -p taskmanager-bevy-ui --lib -j 4 \
    --no-fail-fast >"$OUT/nextest.log" 2>&1
status=$?
set -e
{
    printf 'run_id=%s\n' "$RUN_ID"
    printf 'git_head=%s\n' "$(cat "$OUT/git-head.txt")"
    printf 'worktree_sha256=%s\n' "$(sha256sum "$OUT/git-status.txt" | cut -d' ' -f1)"
    printf 'rust=%s\n' "$(cat "$OUT/rust.txt")"
    printf 'matrix=scripts/parity/cross_frontend_matrix.tsv\n'
    printf 'matrix_projection=%s\n' "target/bevy-interaction-evidence/$RUN_ID/matrix.tsv"
    printf 'matrix_count=%s\n' "$MATRIX_COUNT"
    printf 'command=cargo nextest run %s -p taskmanager-bevy-ui --lib -j 4 --no-fail-fast\n' "$LOCK_FLAG"
    printf 'status=%s\n' "$status"
} >"$OUT/receipt.txt"
{
    printf '{\n'
    printf '  "run_id": "%s",\n' "$RUN_ID"
    printf '  "git_head": "%s",\n' "$(cat "$OUT/git-head.txt")"
    printf '  "worktree_sha256": "%s",\n' "$(sha256sum "$OUT/git-status.txt" | cut -d' ' -f1)"
    printf '  "rust": "%s",\n' "$(cat "$OUT/rust.txt")"
    printf '  "matrix": "scripts/parity/cross_frontend_matrix.tsv",\n'
    printf '  "matrix_count": %s,\n' "$MATRIX_COUNT"
    printf '  "command": "cargo nextest run %s -p taskmanager-bevy-ui --lib -j 4 --no-fail-fast",\n' "$LOCK_FLAG"
    printf '  "status": "%s",\n' "$([ "$status" -eq 0 ] && printf 'pass' || printf 'fail')"
    printf '  "exit_code": %s\n' "$status"
    printf '}\n'
} >"$OUT/receipt.json"
[ "$status" -eq 0 ] || die "Bevy lib target failed (receipt: $OUT/receipt.txt)"
printf 'Bevy interaction gate: PASS (%s matrix cases; full lib target green) -> %s\n' \
    "$MATRIX_COUNT" "$OUT"
