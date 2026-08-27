#!/usr/bin/env bash
# Run the fail-closed Iced interaction acceptance gate.
#
# This is headless acceptance: taskmanager-iced's lib-test suite drives the
# real IcedApp update/view pipeline (IcedApp::demo / with_config_store
# construction, Message dispatch, projection + renderer-state assertions)
# without a Wayland compositor or GPU. Pixel proof stays with
# capture-iced.sh / capture-iced-matrix.sh; neither gate closes an interaction
# requirement alone.
#
# Fail-closed contract (mirrors accept-gpui-interactions.sh):
#   1. scripts/iced_interaction_matrix.tsv covers every public requirement,
#      every requirement carries a success path, and every capture
#      scenario name exists in capture_iced_scenarios.tsv.
#   2. Every matrix test name is present in `cargo nextest list` for the
#      taskmanager-iced lib target.
#   3. The whole lib target runs with the locked workspace and the CI nextest
#      profile (NEXTEST_EXPERIMENTAL_LIBTEST_JSON for per-test events).
#   4. A libtest JSON `ok` event must exist for every matrix test; a missing,
#      filtered, renamed or failed case fails the receipt validator.
#   5. Git/Rust/discovery/run/validation receipts land under
#      target/iced-interaction-evidence/<run>/.
#
# Self-test: `bash scripts/accept-iced-interactions.sh --self-test` exercises
# the embedded validators against synthetic fixtures (a consistent pass plus
# every fail-closed mutation) without touching cargo.
set -euo pipefail
export LC_ALL=C

REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"

MATRIX="$REPO/scripts/iced_interaction_matrix.tsv"
REQUIREMENTS="$REPO/scripts/interaction_requirements.tsv"
CAPTURE_SCENARIOS="$REPO/scripts/capture_iced_scenarios.tsv"

MATRIX_FIELDS=(case_id p0_id target test_name paths capture_scenarios)
ALLOWED_TARGETS=(lib)
ALLOWED_PATHS=(
    cancel evidence failure focus isolation keyboard lifecycle pointer
    provider-gap recovery responsive success toggle
)

die() {
    printf 'Iced interaction gate: FAIL: %s\n' "$1" >&2
    exit 1
}

# ---------------------------------------------------------------------------
# Matrix + requirement + capture-scenario structural validation (pure bash).
# ---------------------------------------------------------------------------
validate_matrix() {
    local matrix="$1" requirements="$2" scenarios="$3"
    [ -s "$matrix" ] || die "matrix is empty or missing: $matrix"

    local header expected_header=""
    header="$(head -n 1 "$matrix")"
    local field
    for field in "${MATRIX_FIELDS[@]}"; do
        expected_header+="${field}"$'\t'
    done
    expected_header="${expected_header%$'\t'}"
    [ "$header" = "$expected_header" ] || die "unexpected matrix header: $header"

    local malformed
    malformed="$(awk -F'\t' 'NR > 1 && $0 != "" && NF != 6 { print NR }' "$matrix")"
    [ -z "$malformed" ] || die "malformed matrix rows (need 6 tab fields): lines $malformed"

    # Public requirement ids are intentionally status-free. Release scoring is
    # kept in local publication material and is not an acceptance dependency.
    local -A requirement_ids=()
    local requirement_count=0
    local line
    while IFS= read -r line; do
        [ -n "$line" ] || continue
        case "$line" in '#'*|requirement_id) continue ;; esac
        [[ "$line" != *$'\t'* ]] || die "malformed requirement row: $line"
        requirement_ids["$line"]=1
        requirement_count=$((requirement_count + 1))
    done <"$requirements"
    [ "$requirement_count" -gt 0 ] || die "no requirement IDs in $requirements"

    # Capture scenario names (column 1 of the iced capture matrix data table).
    local -A scenario_names=()
    while IFS= read -r line; do
        [ -n "$line" ] || continue
        case "$line" in '#'*|name$'\t'*) continue ;; esac
        fields=()
        IFS=$'\t' read -r -a fields <<<"$line"
        [ "${#fields[@]}" -eq 3 ] || die "malformed capture-scenario row: $line"
        scenario_names["${fields[0]}"]=1
    done <"$scenarios"
    [ "${#scenario_names[@]}" -gt 0 ] || die "no capture scenarios in $scenarios"

    local -A seen_cases=() seen_tests=() covered_p0=() success_p0=()
    local case_count=0
    local case_id p0_id target test_name paths scenario_cell
    while IFS=$'\t' read -r case_id p0_id target test_name paths scenario_cell; do
        [ -n "${case_id:-}" ] || die "empty case_id"
        [ -z "${seen_cases[$case_id]+set}" ] || die "duplicate case_id: $case_id"
        seen_cases[$case_id]=1
        [ -n "${requirement_ids[$p0_id]+set}" ] || die "$case_id: unknown requirement ID $p0_id"
        local target_ok=0 allowed
        for allowed in "${ALLOWED_TARGETS[@]}"; do
            if [ "$target" = "$allowed" ]; then target_ok=1; fi
        done
        [ "$target_ok" -eq 1 ] || die "$case_id: invalid target $target"
        [ -n "$test_name" ] || die "$case_id: empty test_name"
        [ -z "${seen_tests[$test_name]+set}" ] || die "duplicate test_name: $test_name"
        seen_tests[$test_name]=1
        [ -n "$paths" ] || die "$case_id: empty paths"
        local path path_ok
        local IFS='|'
        for path in $paths; do
            path_ok=0
            for allowed in "${ALLOWED_PATHS[@]}"; do
                if [ "$path" = "$allowed" ]; then path_ok=1; fi
            done
            [ "$path_ok" -eq 1 ] || die "$case_id: invalid path $path"
            if [ "$path" = success ]; then success_p0[$p0_id]=1; fi
        done
        unset IFS
        if [ "$scenario_cell" != "-" ] && [ -n "$scenario_cell" ]; then
            local scenario
            local IFS='|'
            for scenario in $scenario_cell; do
                [ -n "${scenario_names[$scenario]+set}" ] \
                    || die "$case_id: unknown capture scenario $scenario"
            done
            unset IFS
        fi
        covered_p0[$p0_id]=1
        case_count=$((case_count + 1))
    done < <(tail -n +2 "$matrix")
    [ "$case_count" -gt 0 ] || die "matrix has no data rows"

    local missing_p0=() missing_success=() id
    for id in "${!requirement_ids[@]}"; do
        [ -n "${covered_p0[$id]+set}" ] || missing_p0+=("$id")
        [ -n "${success_p0[$id]+set}" ] || missing_success+=("$id")
    done
    [ "${#missing_p0[@]}" -eq 0 ] || die "requirement IDs missing from matrix: ${missing_p0[*]}"
    [ "${#missing_success[@]}" -eq 0 ] || die "requirements without a success path: ${missing_success[*]}"

    MATRIX_CASE_COUNT="$case_count"
    MATRIX_P0_COUNT="${#covered_p0[@]}"
}

# ---------------------------------------------------------------------------
# Nextest discovery + run-log extraction (one bounded python3 -c per file;
# the logic itself lives committed in this script).
# ---------------------------------------------------------------------------
libtest_names() {
    timeout 30s python3 -c 'import json, sys
payload = json.load(open(sys.argv[1], encoding="utf-8"))
suites = payload.get("rust-suites")
if not isinstance(suites, dict):
    sys.exit("missing rust-suites")
found = []
for suite in suites.values():
    if not isinstance(suite, dict) or suite.get("kind") != "lib":
        continue
    cases = suite.get("testcases")
    if isinstance(cases, dict):
        found.extend(cases)
if not found:
    sys.exit("no lib test cases discovered")
for name in sorted(set(found)):
    print(name)
' "$1" | tr -d '\r'
}

# Emit "event<TAB>test-name" lines from a libtest JSON run log. The suite
# prefix (`taskmanager-iced::taskmanager_iced$`) is stripped; non-JSON cargo
# chatter is ignored.
run_log_events() {
    timeout 30s python3 -c 'import json, sys
for raw in open(sys.argv[1], encoding="utf-8"):
    raw = raw.strip()
    if not raw.startswith("{"):
        continue
    try:
        event = json.loads(raw)
    except json.JSONDecodeError:
        continue
    if event.get("type") != "test":
        continue
    name = event.get("name")
    if not isinstance(name, str) or "$" not in name:
        continue
    print(event.get("event"), name.split("$", 1)[1], sep="\t")
' "$1" | tr -d '\r'
}

matrix_test_names() {
    awk -F'\t' 'NR > 1 && NF == 6 { print $4 }' "$1" | sort -u
}

validate_discovery() {
    local matrix="$1" discovered="$2"
    [ -s "$discovered" ] || die "discovery list is empty or missing"
    local missing
    missing="$(matrix_test_names "$matrix" | comm -23 - <(sort -u "$discovered"))"
    [ -z "$missing" ] || die "matrix tests were not discovered: $(printf '%s ' $missing)"
}

# Every matrix test needs one `ok` event; any `failed` event is fatal.
validate_run() {
    local matrix="$1" events="$2"
    [ -s "$events" ] || die "run event extraction is empty (run log missing JSON events?)"
    local ok_events failures
    ok_events="$(awk -F'\t' '$1 == "ok" { print $2 }' "$events" | sort -u)"
    failures="$(awk -F'\t' '$1 == "failed" { print $2 }' "$events" | sort -u)"
    local missing_ok
    missing_ok="$(matrix_test_names "$matrix" | comm -23 - <(printf '%s\n' "$ok_events" | sort -u))"
    [ -z "$missing_ok" ] || die "interaction receipt incomplete, no ok event: $(printf '%s ' $missing_ok)"
    [ -z "$failures" ] || die "failed test events recorded: $(printf '%s ' $failures)"
}

write_base_matrix() {
    printf 'case_id\tp0_id\ttarget\ttest_name\tpaths\tcapture_scenarios\n' >"$1"
    printf 'xa-case\tP0-XA\tlib\tsuite::alpha\tsuccess|keyboard\tcpu\n' >>"$1"
    printf 'xb-case\tP0-XB\tlib\tsuite::beta\tsuccess|failure\t-\n' >>"$1"
}

# ---------------------------------------------------------------------------
# Self-test: synthetic fixtures prove the validators accept a consistent set
# and reject every fail-closed mutation. Validations that must fail run in a
# subshell so their `exit 1` cannot end the self-test. No cargo involved.
# ---------------------------------------------------------------------------
self_test() {
    dir="$REPO/.tmp/iced-interaction-selftest.$$"
    rm -rf "$dir"
    mkdir -p "$dir"
    trap 'rm -rf "$dir"' EXIT

    printf 'requirement_id\n' >"$dir/requirements.tsv"
    printf 'P0-XA\nP0-XB\n' >>"$dir/requirements.tsv"

    printf 'name\tdevice\twindow_size\n' >"$dir/scenarios.tsv"
    printf 'cpu\tcpu\t1180x780\n' >>"$dir/scenarios.tsv"

    write_base_matrix "$dir/matrix.tsv"

    timeout 30s python3 -c 'import json, sys
payload = {"rust-suites": {"taskmanager-iced": {"kind": "lib",
    "binary-name": "taskmanager_iced",
    "testcases": {"suite::alpha": {"ignored": False},
                  "suite::beta": {"ignored": False}}}}}
json.dump(payload, open(sys.argv[1], "w", encoding="utf-8"))
' "$dir/list.json"

    printf '%s\n' 'suite::alpha' 'suite::beta' >"$dir/discovered.txt"

    printf '%s\n' \
        '{"type":"suite","event":"started","test_count":2}' \
        '{"type":"test","event":"started","name":"taskmanager-iced::taskmanager_iced$suite::alpha"}' \
        '{"type":"test","event":"ok","name":"taskmanager-iced::taskmanager_iced$suite::alpha"}' \
        '{"type":"test","event":"started","name":"taskmanager-iced::taskmanager_iced$suite::beta"}' \
        '{"type":"test","event":"ok","name":"taskmanager-iced::taskmanager_iced$suite::beta"}' \
        >"$dir/run.log"

    local events="$dir/events.txt" list="$dir/list-from-json.txt"
    run_log_events "$dir/run.log" >"$events"
    libtest_names "$dir/list.json" >"$list"

    # Happy path: every validator accepts the consistent fixture set.
    validate_matrix "$dir/matrix.tsv" "$dir/requirements.tsv" "$dir/scenarios.tsv"
    validate_discovery "$dir/matrix.tsv" "$list"
    validate_run "$dir/matrix.tsv" "$events"
    [ "$MATRIX_CASE_COUNT" -eq 2 ] || die "self-test: case count mismatch"
    [ "$MATRIX_P0_COUNT" -eq 2 ] || die "self-test: p0 count mismatch"
    [ "$(grep -c . "$list")" -eq 2 ] || die "self-test: discovery extraction mismatch"
    [ "$(grep -c . "$events")" -eq 4 ] || die "self-test: run-log extraction mismatch"

    # Mutation 1: unknown parity id must be rejected.
    printf 'xc-case\tP0-XC\tlib\tsuite::gamma\tsuccess\t-\n' >>"$dir/matrix.tsv"
    if ( validate_matrix "$dir/matrix.tsv" "$dir/requirements.tsv" "$dir/scenarios.tsv" ) 2>/dev/null; then
        die "self-test: unknown requirement ID must be rejected"
    fi
    write_base_matrix "$dir/matrix.tsv"

    # Mutation 2: a requirement without a success path must be rejected.
    printf 'P0-XC\n' >>"$dir/requirements.tsv"
    printf 'xc-case\tP0-XC\tlib\tsuite::gamma\tfailure\t-\n' >>"$dir/matrix.tsv"
    if ( validate_matrix "$dir/matrix.tsv" "$dir/requirements.tsv" "$dir/scenarios.tsv" ) 2>/dev/null; then
        die "self-test: requirement without success path must be rejected"
    fi
    write_base_matrix "$dir/matrix.tsv"

    # Mutation 3: an unknown capture scenario must be rejected.
    printf 'case_id\tp0_id\ttarget\ttest_name\tpaths\tcapture_scenarios\n' >"$dir/matrix.tsv"
    printf 'xa-case\tP0-XA\tlib\tsuite::alpha\tsuccess|keyboard\tnope\n' >>"$dir/matrix.tsv"
    printf 'xb-case\tP0-XB\tlib\tsuite::beta\tsuccess|failure\t-\n' >>"$dir/matrix.tsv"
    if ( validate_matrix "$dir/matrix.tsv" "$dir/requirements.tsv" "$dir/scenarios.tsv" ) 2>/dev/null; then
        die "self-test: unknown capture scenario must be rejected"
    fi
    write_base_matrix "$dir/matrix.tsv"

    # Mutation 4: a matrix test missing from discovery must be rejected.
    printf '%s\n' 'suite::alpha' >"$dir/discovered-partial.txt"
    if ( validate_discovery "$dir/matrix.tsv" "$dir/discovered-partial.txt" ) 2>/dev/null; then
        die "self-test: undiscovered matrix test must be rejected"
    fi

    # Mutation 5: a run log without the ok event must be rejected.
    grep -v 'suite::beta' "$dir/events.txt" >"$dir/events-missing.txt"
    if ( validate_run "$dir/matrix.tsv" "$dir/events-missing.txt" ) 2>/dev/null; then
        die "self-test: missing ok event must be rejected"
    fi

    # Mutation 6: a failed matrix test must be rejected.
    printf '%s\n' 'failed	suite::beta' >>"$dir/events.txt"
    if ( validate_run "$dir/matrix.tsv" "$dir/events" ) 2>/dev/null; then
        die "self-test: failed test event must be rejected"
    fi

    rm -rf "$dir"
    trap - EXIT
    printf 'Iced interaction gate self-test: PASS\n'
}

if [ "${1:-}" = "--self-test" ]; then
    self_test
    exit 0
fi

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

eval "$(scripts/agent-workdir.sh enter iced-interactions)"
export CARGO_BUILD_JOBS=4

RUN_STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
GIT_HEAD="$(git rev-parse --short=12 HEAD 2>/dev/null || printf 'no-git')"
WORKTREE_STATE=clean
if [ -n "$(git status --porcelain)" ]; then
    WORKTREE_STATE=dirty
fi
RUN_ID="${RUN_STAMP}_${GIT_HEAD}_${WORKTREE_STATE}_$$"
RUN_DIR="$REPO/target/iced-interaction-evidence/$RUN_ID"
mkdir -p "$RUN_DIR"

printf 'run_id=%s\n' "$RUN_ID" >"$RUN_DIR/metadata.txt"
printf 'git_head=%s\n' "$GIT_HEAD" >>"$RUN_DIR/metadata.txt"
printf 'worktree=%s\n' "$WORKTREE_STATE" >>"$RUN_DIR/metadata.txt"
printf 'rust=%s\n' "$(rustc -V)" >>"$RUN_DIR/metadata.txt"
printf 'command=bash scripts/accept-iced-interactions.sh\n' >>"$RUN_DIR/metadata.txt"

# 1) Validators prove themselves before they judge the real inputs.
self_test | tee "$RUN_DIR/self-test.log"

# 2) Matrix structure + ledger coverage + capture-scenario names.
validate_matrix "$MATRIX" "$REQUIREMENTS" "$CAPTURE_SCENARIOS"
printf 'case_count=%s\np0_count=%s\n' "$MATRIX_CASE_COUNT" "$MATRIX_P0_COUNT" \
    >"$RUN_DIR/matrix-summary.txt"

# 3) Every matrix test must exist in nextest's discovery of the lib target.
timeout --kill-after=10s 20m cargo nextest list --locked --profile ci \
    -p taskmanager-iced --lib --message-format json >"$RUN_DIR/lib-list.json"
LIBTEST_NAMES="$RUN_DIR/lib-list-names.txt"
libtest_names "$RUN_DIR/lib-list.json" >"$LIBTEST_NAMES"
validate_discovery "$MATRIX" "$LIBTEST_NAMES"

# 4) Run the whole taskmanager-iced lib target with per-test JSON events.
NEXTEST_EXPERIMENTAL_LIBTEST_JSON=1 \
    timeout --kill-after=10s 30m cargo nextest run --locked --profile ci \
    --message-format libtest-json-plus --cargo-quiet -j 4 \
    -p taskmanager-iced --lib >"$RUN_DIR/lib-run.log" 2>&1

RUN_EVENTS="$RUN_DIR/lib-run-events.txt"
run_log_events "$RUN_DIR/lib-run.log" >"$RUN_EVENTS"
validate_run "$MATRIX" "$RUN_EVENTS"

# 5) Receipt: numbers come from the validated artifacts, never expectations.
{
    printf '{\n'
    printf '  "status": "pass",\n'
    printf '  "matrix": {"case_count": %s, "p0_count": %s},\n' \
        "$MATRIX_CASE_COUNT" "$MATRIX_P0_COUNT"
    printf '  "discovered_lib_tests": %s,\n' "$(grep -c . "$LIBTEST_NAMES")"
    printf '  "run": {"ok_events": %s, "failed_events": %s, "test_events": %s}\n' \
        "$(awk -F'\t' '$1 == "ok" { n += 1 } END { print n + 0 }' "$RUN_EVENTS")" \
        "$(awk -F'\t' '$1 == "failed" { n += 1 } END { print n + 0 }' "$RUN_EVENTS")" \
        "$(grep -c . "$RUN_EVENTS")"
    printf '}\n'
} >"$RUN_DIR/interaction-validation.json"

printf 'Iced interaction acceptance: PASS\n'
printf 'receipt=%s\n' "$RUN_DIR/interaction-validation.json"
