#!/usr/bin/env bash
# local-gates.sh — simulate the CI gate set locally.
#
# GitHub Actions free quota is a finite budget, so the expensive and
# environment-dependent checks moved home. This script is the single entry
# point that reproduces them on this machine, tiered by cost:
#
#   quick      seconds-minutes: fmt, python policy gates, install-manager
#              smoke, line/doc/test-layout/source-inspection/bevy-bsn/headless
#              side-effect guards, per-crate coverage gate self-test.
#   standard   ~ the old Linux CI job: quick + cargo deny + clippy + the
#              nextest workspace split into core/logic/gui/perf layers
#              (failure attribution per layer; `--only nextest-core` gives a
#              bottom-up dev loop) + doctests + rustdoc + the nvidia fallback
#              matrix + release build + (with --with-gui) the GPUI interaction
#              matrix on a live Wayland session.
#   extended   the expensive pass: llvm-cov with per-crate floors, mutation
#              testing of the core/application diff, Miri on the three
#              audited unsafe crates, fuzz-target build (+ runs on demand),
#              release bloat and bench trends vs docs/quality/*-trend.tsv.
#
# Every stage is bounded by an outer `timeout --kill-after=` deadline and
# the result is recorded per stage; the script exits non-zero if any stage
# failed. Scratch space lives under .tmp/ (repo NVMe, gitignored), never
# /tmp, and is removed on exit. Parallelism caps at CARGO_BUILD_JOBS (4)
# so interactive work is not starved.
#
# Usage:
#   scripts/quality/local-gates.sh [quick|standard|extended] [--with-gui]
#     [--with-fuzz-runs] [--skip-release] [--only <stage>]
#   No tier argument runs `quick`.
#
# Environment overrides:
#   JOBS=<n>                cargo/test parallelism (default 4)
#   STAGE_TIMEOUT=<sec>     per-stage deadline (default 3600)
#   QUICK_TIMEOUT=<sec>     quick-stage deadline (default 300)

set -u

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"

export CARGO_BUILD_JOBS="${JOBS:-4}"
export TMPDIR="$repo/.tmp/local-gates-$$-$(date +%s)/tmp"
scratch="${TMPDIR%/tmp}"
mkdir -p "$TMPDIR"

stage_timeout="${STAGE_TIMEOUT:-3600}"
quick_timeout="${QUICK_TIMEOUT:-300}"
results_file="$scratch/results.tsv"
failures=0
stages_run=0
started_at="$(date +%s)"

record() {
    printf '%s\t%s\t%s\t%s\n' "$1" "$2" "$3" "$4" >>"$results_file"
}

run_stage() {
    local name="$1"
    local tier="$2"
    shift 2
    local deadline="$quick_timeout"
    [[ "$tier" != "quick" ]] && deadline="$stage_timeout"
    local started
    started="$(date +%s)"
    echo ""
    echo "=== $name ==="
    local rc=0
    if [[ "${1:-}" == "run_py" ]]; then
        # run_py is a shell function: it applies its own timeout, so an
        # external `timeout` wrapper cannot invoke it directly.
        shift
        run_py "$@" || rc=$?
    else
        timeout --kill-after=30s "$deadline" "$@" || rc=$?
    fi
    if [[ $rc -eq 0 ]]; then
        record "$name" "$tier" "PASS" "$(( $(date +%s) - started ))"
        echo "PASS $name"
    else
        record "$name" "$tier" "FAIL" "$(( $(date +%s) - started ))"
        echo "FAIL $name"
        failures=$((failures + 1))
    fi
    stages_run=$((stages_run + 1))
}

run_py() {
    timeout --kill-after=10s "${PY_TIMEOUT:-120}" python3 "$@"
}

only="${ONLY_STAGE:-}"

maybe() {
    if [[ -n "$only" && "$only" != "$1" ]]; then
        echo "SKIP $1 (--only $only)"
        record "$1" "quick" "SKIP" 0
        return 1
    fi
    return 0
}

with_gui=0
with_fuzz_runs=0
skip_release=0
tier="quick"
while [[ $# -gt 0 ]]; do
    case "$1" in
    quick | standard | extended) tier="$1" ;;
    --with-gui) with_gui=1 ;;
    --with-fuzz-runs) with_fuzz_runs=1 ;;
    --skip-release) skip_release=1 ;;
    --only)
        shift
        ONLY_STAGE="$1"
        only="$1"
        ;;
    *)
        echo "unknown argument '$1'" >&2
        exit 2
        ;;
    esac
    shift
done

cleanup() {
    local rc=$?
    echo ""
    echo "=== local-gates summary ($tier, $stages_run stages, $failures failed) ==="
    if [[ -f "$results_file" ]]; then
        column -t -s $'\t' "$results_file" 2>/dev/null || cat "$results_file"
    fi
    if [[ $failures -gt 0 ]]; then
        echo "local-gates: $failures stage(s) failed (took $(( $(date +%s) - started_at ))s)"
    else
        echo "local-gates: clean (took $(( $(date +%s) - started_at ))s)"
    fi
    rm -rf "$scratch"
    exit "$rc"
}
trap cleanup EXIT INT TERM

case "$tier" in
quick | standard | extended) ;;
*)
    echo "unknown tier '$tier' (quick|standard|extended)" >&2
    exit 2
    ;;
esac

# ---- quick -----------------------------------------------------------
if maybe fmt; then
    run_stage fmt quick cargo fmt --all -- --check
fi
if maybe safety-guard-self; then
    run_stage safety-guard-self quick run_py scripts/quality/automation_safety_guard.py --self-test
fi
if maybe safety-guard; then
    run_stage safety-guard quick run_py scripts/quality/automation_safety_guard.py
fi
if maybe public-repo-self; then
    run_stage public-repo-self quick run_py scripts/quality/public_repo_guard.py --self-test
fi
if maybe public-repo; then
    run_stage public-repo quick run_py scripts/quality/public_repo_guard.py
fi
if maybe install-manifest-self; then
    run_stage install-manifest-self quick run_py scripts/quality/system_install_manifest_guard.py --self-test
fi
if maybe install-manifest; then
    run_stage install-manifest quick run_py scripts/quality/system_install_manifest_guard.py
fi
if maybe visual-capture-coverage; then
    run_stage visual-capture-coverage quick run_py scripts/quality/visual_capture_coverage.py --repo-root "$repo"
fi
if maybe install-manager-smoke; then
    run_stage install-manager-smoke quick timeout --kill-after=10s 30s scripts/test-system-install-manager.sh
fi
if maybe line-guard; then
    run_stage line-guard quick run_py scripts/quality/rust_line_guard.py --mode enforce
fi
if maybe rust-surface-guard-self; then
    run_stage rust-surface-guard-self quick run_py scripts/quality/rust_surface_guard.py --self-test
fi
if maybe rust-surface-guard; then
    run_stage rust-surface-guard quick run_py scripts/quality/rust_surface_guard.py --mode enforce
fi
if maybe bevy-bsn-guard-self; then
    run_stage bevy-bsn-guard-self quick run_py scripts/quality/bevy_bsn_guard.py --self-test
fi
if maybe bevy-bsn-guard; then
    run_stage bevy-bsn-guard quick run_py scripts/quality/bevy_bsn_guard.py --mode enforce
fi
if maybe test-layout-self; then
    run_stage test-layout-self quick run_py scripts/quality/test_layout_guard.py --self-test
fi
if maybe test-layout-enforce; then
    run_stage test-layout-enforce quick run_py scripts/quality/test_layout_guard.py --mode enforce
fi
if maybe source-inspection-self; then
    run_stage source-inspection-self quick run_py scripts/quality/source_inspection_guard.py --self-test
fi
if maybe source-inspection-enforce; then
    run_stage source-inspection-enforce quick run_py scripts/quality/source_inspection_guard.py --mode enforce
fi
if maybe headless-side-effect-self; then
    run_stage headless-side-effect-self quick run_py scripts/quality/headless_side_effect_guard.py --self-test
fi
if maybe headless-side-effect-enforce; then
    run_stage headless-side-effect-enforce quick run_py scripts/quality/headless_side_effect_guard.py --mode enforce
fi
if maybe doc-guard; then
    run_stage doc-guard quick run_py scripts/quality/module_doc_guard.py --mode enforce
fi
if maybe doc-governance-self; then
    run_stage doc-governance-self quick run_py scripts/quality/doc_governance_guard.py --self-test
fi
if maybe doc-governance; then
    run_stage doc-governance quick run_py scripts/quality/doc_governance_guard.py
fi
if maybe coverage-gate-self; then
    run_stage coverage-gate-self quick run_py scripts/quality/per_crate_coverage_gate.py --self-test
fi
if maybe coverage-gate; then
    run_stage coverage-gate quick run_py scripts/quality/per_crate_coverage_gate.py --lcov target/lcov.info --check
fi

[[ "$tier" == "quick" ]] && exit "$((failures > 0))"

# ---- standard --------------------------------------------------------
if maybe deny; then
    run_stage deny standard timeout --kill-after=30s 600 cargo deny check
fi
if maybe clippy; then
    # `test-support` stays DEV-only: it enables gpui's upstream test-support
    # (which hard-wires X11) so headless GPUI tests compile; production
    # builds never enable it (strict Wayland product policy).
    run_stage clippy standard cargo clippy --locked --workspace --all-targets --features test-support -- \
        -D warnings -W clippy::cognitive_complexity -W clippy::too_many_lines
fi
if maybe nextest-core; then
    # Unit tests across every workspace crate plus non-root integration
    # targets (platform contract proofs, adapter tests, ...). The three root
    # integration binaries are partitioned into their own layers below so a
    # failing layer is attributable without re-running the whole suite.
    # nextest 0.9 exposes `-j` and `--test-threads` as aliases for the same
    # option; pass it once so the gate remains valid across the installed
    # runner instead of triggering a duplicate-argument parse failure.
    run_stage nextest-core standard cargo nextest run --locked --workspace --all-targets --features test-support -j 4 --profile ci -E 'not (binary(throughput) or binary(logic) or binary(gui) or binary(performance))'
fi
if maybe nextest-logic; then
    run_stage nextest-logic standard cargo nextest run --locked -p taskmanager --test logic --features test-support -j 4 --profile ci
fi
if maybe nextest-gui; then
    run_stage nextest-gui standard cargo nextest run --locked -p taskmanager --test gui --features test-support -j 4 --profile ci
fi
if maybe nextest-perf; then
    run_stage nextest-perf standard cargo nextest run --locked -p taskmanager --test performance --features test-support -j 4 --profile ci
fi
if maybe live-smoke; then
    # One real-collector tick per supported platform (host-neutral invariants
    # only). Fixtures prove parsers; this stage proves the composition edge.
    run_stage live-smoke standard cargo nextest run --locked -p taskmanager --test logic --features test-support -j 4 --profile ci -E 'test(live_smoke_)'
fi
if maybe ui-route; then
    # UI diffs must carry the headless GUI matrix: pure core changes skip it.
    if [[ "$with_gui" == "1" ]]; then
        run_stage ui-route standard bash scripts/quality/ui-evidence-route.sh --with-gui
    else
        run_stage ui-route standard bash scripts/quality/ui-evidence-route.sh
    fi
fi
if maybe doctests; then
    run_stage doctests standard cargo test --locked --doc --workspace
fi
if maybe rustdoc; then
    run_stage rustdoc standard env RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
fi
if maybe nvidia-fallback; then
    run_stage nvidia-fallback standard cargo nextest run --locked --workspace --lib --no-default-features --features hardware-all,nvidia,ui-gpui -j 4 --profile ci
fi
if maybe shape-tui; then
    run_stage shape-tui standard bash -c \
        'cargo check --locked --workspace --all-targets --no-default-features --features hardware-all,ui-tui && cargo nextest run --locked --workspace --all-targets --no-default-features --features hardware-all,ui-tui -j 4 --profile ci -E "not binary(throughput)"'
fi
if maybe shape-iced; then
    run_stage shape-iced standard bash -c \
        'cargo check --locked --workspace --all-targets --no-default-features --features hardware-all,ui-iced && cargo nextest run --locked --workspace --all-targets --no-default-features --features hardware-all,ui-iced -j 4 --profile ci -E "not binary(throughput)"'
fi
if [[ "$skip_release" == "1" ]]; then
    record release standard SKIP 0
else
    if maybe release; then
        run_stage release standard cargo build --locked --release
    fi
fi
if [[ "$with_gui" == "1" ]]; then
    if maybe ui-capture-route; then
        # Capture acceptance additionally requires fresh pixel receipts for
        # every touched frontend shape (run the selected capture workflow).
        run_stage ui-capture-route standard bash scripts/quality/ui-evidence-route.sh --with-gui --require-capture
    fi
    if maybe gpui-interactions; then
        run_stage gpui-interactions standard timeout --kill-after=10s 2400 bash scripts/accept-gpui-interactions.sh
    fi
fi

[[ "$tier" == "standard" ]] && exit "$((failures > 0))"

# ---- extended --------------------------------------------------------
if maybe coverage; then
    run_stage coverage extended bash -c \
        'cargo llvm-cov nextest --locked --workspace --all-targets --features test-support --profile ci -E "not binary(throughput)" --lcov --output-path target/lcov.info --fail-under-lines 71 && timeout --kill-after=10s 120 python3 scripts/quality/per_crate_coverage_gate.py --lcov target/lcov.info --check'
fi
if maybe mutants; then
    # Decision logic in core/application plus the Linux fixture parsers: all
    # three are the layers whose silent breakage the behavior gates must catch.
    run_stage mutants extended scripts/quality/mutants-in-diff.sh --packages taskmanager-core,taskmanager-application,taskmanager-platform-linux --min-score 80
fi
if maybe miri; then
    run_stage miri extended scripts/quality/miri-boundaries.sh
fi
if maybe fuzz-build; then
    # cargo-fuzz 0.13.x rejects --manifest-path: each fuzz workspace is built
    # from its own directory (they are deliberately not workspace members).
    run_stage fuzz-build extended bash -c \
        'cd crates/taskmanager-afpacket/fuzz && cargo +nightly fuzz build && cd ../../taskmanager-platform-linux/fuzz && cargo +nightly fuzz build && cd ../../../crates/taskmanager-fd-bridge/fuzz && cargo +nightly fuzz build'
fi
if [[ "$with_fuzz_runs" == "1" ]]; then
    if maybe fuzz-run; then
        run_stage fuzz-run extended bash -c \
            'cd crates/taskmanager-afpacket/fuzz && cargo +nightly fuzz run five_tuple -- -runs=2000000 -timeout=5 && cd ../../taskmanager-platform-linux/fuzz && cargo +nightly fuzz run proc_parsers -- -runs=2000000 -timeout=5 && cargo +nightly fuzz run mm_stat -- -runs=2000000 -timeout=5 && cd ../../../crates/taskmanager-fd-bridge/fuzz && cargo +nightly fuzz run scm_rights_walk -- -runs=2000000 -timeout=5'
    fi
fi
if maybe bloat; then
    run_stage bloat extended bash -c \
        '[[ -f target/release/taskmanager ]] || cargo build --locked --release; scripts/quality/trend-gate.sh --metric bloat --current "$(stat -c %s target/release/taskmanager 2>/dev/null || echo 0)" --trend docs/quality/bloat-trend.tsv --limit 5'
fi
if maybe benches; then
    run_stage benches extended scripts/quality/bench-gate.sh
fi

exit "$((failures > 0))"
