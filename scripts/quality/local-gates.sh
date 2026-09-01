#!/usr/bin/env bash
# local-gates.sh — simulate the CI gate set locally.
#
# GitHub Actions free quota is a finite budget, so the expensive and
# environment-dependent checks moved home. This script is the single entry
# point that reproduces them on this machine, tiered by cost:
#
#   quick      seconds-minutes: toolchain/platform preflight, fmt, dependency-floor,
#              python policy gates, test-runner policy, install-manager
#              smoke, line/doc/test-layout/source-inspection/bevy-bsn/headless
#              side-effect guards, per-crate coverage gate self-test, and the
#              private A/B capture-isolation check when a host Wayland session
#              is available (or TM_CAPTURE_ISOLATION_GATE=1 forces it).
#   standard   ~ the blocking Linux CI surface: quick + cargo deny + clippy + the
#              nextest workspace split into core/logic/gui/perf layers
#              (failure attribution per layer; `--only nextest-core` gives a
#              bottom-up dev loop) + doctests + rustdoc + the nvidia fallback
#              matrix + release/package smoke + (with --with-gui) the GPUI
#              interaction matrix and fresh capture receipt route.
#   extended   the expensive pass: llvm-cov with per-crate floors, mutation
#              testing of the core/application diff, Miri on the three
#              Linux-audited unsafe crates, fuzz-target build (+ runs on demand),
#              release bloat and bench trends vs docs/quality/*-trend.tsv.
#
# Every stage is bounded by an outer `timeout --kill-after=` deadline and
# the result is recorded per stage. The default is fail-fast: the first failed
# stage exits immediately; use `--keep-going` only when collecting a diagnostic
# batch. Scratch space lives under .agent-scratch/ beside the repo (same NVMe,
# never inside $repo: a scratch in the tree gets copied into its own copy by
# tools that snapshot sources — cargo-mutants — and nests recursively), is
# swept at startup for runs leaked by hard kills, and is removed on exit.
# Parallelism caps at CARGO_BUILD_JOBS (4) so interactive work is not starved.
#
# Usage:
#   scripts/quality/local-gates.sh [quick|standard|extended] [--with-gui]
#     [--with-fuzz-runs] [--skip-release] [--keep-going] [--only <stage>]
#     [--scope <all|core|bevy|gpui|iced|tui>]
#   No tier argument runs `quick`.
#
# --scope is the parallel-work isolation: a frontend line develops against
# "core + its dependency closure + its own crate" and never fails on another
# line's in-flight files. The package set is derived at runtime from
# `cargo tree` (the frontend crate's workspace closure), never hand-maintained.
# `all` (default) is the merge-owner surface: every workspace crate, plus the
# root acceptance layers and release smoke.
#
# Environment overrides:
#   JOBS=<n>                cargo/test parallelism (default 4)
#   STAGE_TIMEOUT=<sec>     per-stage deadline (default 3600)
#   QUICK_TIMEOUT=<sec>     quick-stage deadline (default 300)
#   TM_SCRATCH_ROOT=<dir>   scratch root (default: .agent-scratch beside the
#                           repo; it must be outside the repository)
#   SKIP_INSTALL_MANAGER_SMOKE=1  defer that smoke until the release build
#                                 has produced its helper binaries
#   TM_CAPTURE_ISOLATION_GATE=auto|0|1  run the real private A/B capture test;
#                                 auto runs it only on a ready Wayland host,
#                                 1 fails closed when its prerequisites are absent

set -u

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"

export CARGO_BUILD_JOBS="${JOBS:-4}"
scratch_root="${TM_SCRATCH_ROOT:-$(dirname "$repo")/.agent-scratch}"
if ! mkdir -p "$scratch_root" 2>/dev/null; then
    echo "local-gates: scratch root '$scratch_root' is not writable;" \
        "set TM_SCRATCH_ROOT to a writable path OUTSIDE the repository" >&2
    exit 2
fi
# Reap runs leaked by hard kills; the EXIT trap cannot fire on SIGKILL.
find "$scratch_root" -maxdepth 1 -name 'local-gates-*' -mtime +2 -exec rm -rf {} + 2>/dev/null
export TMPDIR="$scratch_root/local-gates-$$-$(date +%s)/tmp"
scratch="${TMPDIR%/tmp}"
mkdir -p "$TMPDIR"

# Keep local Cargo invocations on the same moving stable channel and warning
# policy as CI. The repository's rust-version remains a compatibility floor;
# it is not the toolchain selected for the current gate run.
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-line-tables-only}"
rustflags="${RUSTFLAGS:-}"
if [[ "$rustflags" != *"-D warnings"* ]]; then
    rustflags="${rustflags:+$rustflags }-D warnings"
fi
if command -v mold >/dev/null 2>&1 && [[ "$rustflags" != *"-fuse-ld=mold"* ]]; then
    rustflags="${rustflags:+$rustflags }-C link-arg=-fuse-ld=mold"
fi
export RUSTFLAGS="$rustflags"

stage_timeout="${STAGE_TIMEOUT:-3600}"
quick_timeout="${QUICK_TIMEOUT:-300}"
skip_install_manager_smoke="${SKIP_INSTALL_MANAGER_SMOKE:-0}"
case "$skip_install_manager_smoke" in
    0|1) ;;
    *)
        echo "SKIP_INSTALL_MANAGER_SMOKE must be 0 or 1" >&2
        exit 2
        ;;
esac
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
        if [[ "$fail_fast" == "1" ]]; then
            echo "local-gates: fail-fast after $name (rc=$rc); use --keep-going for a diagnostic batch" >&2
            exit "$rc"
        fi
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
fail_fast=1
scope="${GATE_SCOPE:-all}"
[[ "${KEEP_GOING:-0}" == "1" ]] && fail_fast=0
tier="quick"
while [[ $# -gt 0 ]]; do
    case "$1" in
    quick | standard | extended) tier="$1" ;;
    --with-gui) with_gui=1 ;;
    --with-fuzz-runs) with_fuzz_runs=1 ;;
    --skip-release) skip_release=1 ;;
    --keep-going) fail_fast=0 ;;
    --only)
        shift
        ONLY_STAGE="$1"
        only="$1"
        ;;
    --scope)
        shift
        scope="${1:-}"
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
    echo "=== local-gates summary ($tier, scope=$scope, $stages_run stages, $failures failed) ==="
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

# ---- scope resolution -------------------------------------------------
# A frontend scope is the frontend crate's forward workspace dependency
# closure ("core + the platform chain + that UI") derived from the lockfile —
# the same set the tier would compile for the crate, and never a hand-copy.
scope_roots_for() {
    case "$scope" in
    core) printf '%s\n' taskmanager-core taskmanager-application taskmanager-shell ;;
    bevy) printf '%s\n' taskmanager-bevy-ui ;;
    gpui) printf '%s\n' taskmanager-gpui taskmanager-ui ;;
    iced) printf '%s\n' taskmanager-iced ;;
    tui) printf '%s\n' taskmanager-tui ;;
    esac
}

SCOPE_PKGS=()
if [[ "$scope" != "all" ]]; then
    case "$scope" in
    all | core | bevy | gpui | iced | tui) ;;
    *)
        echo "unknown scope '$scope' (all|core|bevy|gpui|iced|tui)" >&2
        exit 2
        ;;
    esac
    derived="$(scope_roots_for | while read -r root; do
        cargo tree --locked -e normal --prefix none -p "$root" 2>/dev/null |
            awk '{print $1}'
    done | sort -u | grep '^taskmanager-' || true)"
    if [[ -z "$derived" ]] && ! cargo metadata --locked --format-version 1 >/dev/null 2>&1; then
        # Lock churn mid-derivation: retry unlocked so the scope still gets
        # its full closure (dev-phase fallback; the probe below reports it).
        derived="$(scope_roots_for | while read -r root; do
            cargo tree -e normal --prefix none -p "$root" 2>/dev/null |
                awk '{print $1}'
        done | sort -u | grep '^taskmanager-' || true)"
    fi
    SCOPE_PKGS=($(printf '%s\n%s\n' "$derived" "$(scope_roots_for)" |
        sort -u | grep '^taskmanager-' || true))
    if [[ ${#SCOPE_PKGS[@]} -eq 0 ]]; then
        echo "scope '$scope' derived an empty package set; the lockfile may be stale" >&2
        exit 2
    fi
    echo "scope: $scope -> ${SCOPE_PKGS[*]}"
fi

scope_pkgs_args() {
    local pkg
    for pkg in "${SCOPE_PKGS[@]}"; do
        printf '%s\n' -p "$pkg"
    done
}

# Repo-wide stages that only make sense for the merge-owner surface: a scope
# records them as SKIP instead of failing on another line's in-flight work.
scope_skip() {
    [[ "$scope" != "all" ]] || return 0
    echo "SKIP $1 (scope=$scope: $2)"
    record "$1" "$3" "SKIP" 0
    return 1
}

# Cargo lock policy: --locked by repo law. The lock-consistency probe below
# may downgrade this run to unlocked (dev-phase fallback) when a sibling
# line keeps the shared lock mid-write; child scripts honor the same switch
# through TM_CARGO_LOCK (set-but-empty = unlocked, unset = --locked).
LOCK_ARGS=(--locked)


preflight() {
    local missing=0
    local command
    case "$(uname -s)" in
    Linux*) ;;
    *)
        echo "this gate entry is Linux-only (uname: $(uname -s)); on Windows use scripts/windows/local-gates.sh" >&2
        missing=1
        ;;
    esac
    for command in cargo rustc git timeout install mktemp stat; do
        if ! command -v "$command" >/dev/null 2>&1; then
            echo "required command is unavailable: $command" >&2
            missing=1
        fi
    done
    if ! timeout 5s python3 --version >/dev/null 2>&1; then
        echo "Python 3 interpreter is not runnable" >&2
        missing=1
    fi
    if command -v cargo >/dev/null 2>&1 && ! cargo fmt --version >/dev/null 2>&1; then
        echo "cargo fmt is unavailable (install the rustfmt component)" >&2
        missing=1
    fi
    printf 'toolchain: %s\n' "$(rustc -V 2>/dev/null || printf 'unavailable')"
    if [[ "$rustflags" == *"-fuse-ld=mold"* ]] && ! command -v mold >/dev/null 2>&1; then
        echo "note: RUSTFLAGS requests mold, but mold is not installed yet; compile stages must install it"
    elif command -v mold >/dev/null 2>&1; then
        echo "linker: mold (CI-compatible accelerator enabled)"
    else
        echo "note: mold is unavailable; using the platform default linker"
    fi
    return "$missing"
}

echo ""
echo "=== preflight ==="
if preflight; then
    record preflight quick PASS 0
    stages_run=$((stages_run + 1))
    echo "PASS preflight"
else
    record preflight quick FAIL 0
    stages_run=$((stages_run + 1))
    failures=1
    echo "FAIL preflight" >&2
    exit 2
fi

# ---- quick -----------------------------------------------------------
if maybe lock-consistency; then
    # A shared-workspace hazard: another line mid-dependency-change leaves
    # the lock inconsistent with some member manifest, which fails every
    # `--locked` cargo stage with an opaque lock error. Probe with a bounded
    # settle window (two consecutive consistent reads) so a short sibling
    # lock cycle is absorbed and a sustained one is reported with explicit
    # attribution instead of failing three stages in with an opaque error.
    lock_settled=0
    consistent_streak=0
    for _ in $(seq 1 6); do
        if cargo metadata --locked --format-version 1 >/dev/null 2>&1; then
            consistent_streak=$((consistent_streak + 1))
        else
            consistent_streak=0
        fi
        [ "$consistent_streak" -ge 2 ] && lock_settled=1 && break
        sleep 15
    done
    if [[ "$lock_settled" == "1" ]]; then
        run_stage lock-consistency quick true
    else
        # Dev-phase fallback (owner-approved): a sibling line keeps the
        # shared lock mid-write, so this run proceeds WITHOUT --locked
        # instead of failing on lock churn. Loud, never silent — the
        # summary records it as FALLBACK, and the merge-owner re-runs the
        # locked surface before merging.
        echo "lock stayed inconsistent through the settle window —" >&2
        echo "DEV-PHASE FALLBACK: cargo stages run UNLOCKED this run" >&2
        LOCK_ARGS=()
        export TM_CARGO_LOCK=""
        record lock-consistency quick FALLBACK 0
        stages_run=$((stages_run + 1))
    fi
fi

if maybe fmt; then
    if [[ "$scope" == "all" ]]; then
        run_stage fmt quick cargo fmt --all -- --check
    else
        # mapfile -t would be cleaner but is a bash 4 feature the Windows
        # mirror also relies on; a plain array assignment keeps both in step.
        SCOPE_FMT_ARGS=()
        for pkg in "${SCOPE_PKGS[@]}"; do
            SCOPE_FMT_ARGS+=(-p "$pkg")
        done
        run_stage fmt quick cargo fmt "${SCOPE_FMT_ARGS[@]}" -- --check
    fi
fi
if maybe safety-guard-self; then
    run_stage safety-guard-self quick run_py scripts/quality/automation_safety_guard.py --self-test
fi
if maybe safety-guard; then
    run_stage safety-guard quick run_py scripts/quality/automation_safety_guard.py
fi
if maybe test-runner-self; then
    run_stage test-runner-self quick run_py scripts/quality/test_runner_guard.py --self-test
fi
if maybe test-runner; then
    run_stage test-runner quick run_py scripts/quality/test_runner_guard.py
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
if [[ "$skip_install_manager_smoke" == "1" ]]; then
    echo "SKIP install-manager-smoke (release helper build is owned by a later gate)"
    record install-manager-smoke quick SKIP 0
elif maybe install-manager-smoke; then
    run_stage install-manager-smoke quick timeout --kill-after=10s 30s scripts/test-system-install-manager.sh
fi
if maybe dependency-floor-self; then
    run_stage dependency-floor-self quick run_py scripts/quality/dependency_floor_guard.py --self-test
fi
if maybe dependency-floor; then
    run_stage dependency-floor quick run_py scripts/quality/dependency_floor_guard.py
fi
if maybe line-guard; then
    run_stage line-guard quick run_py scripts/quality/rust_line_guard.py --mode enforce
fi
if maybe rust-surface-guard-self; then
    run_stage rust-surface-guard-self quick run_py scripts/quality/rust_surface_guard.py --self-test
fi
if maybe rust-surface-guard; then
    if [[ "$scope" == "all" ]]; then
        run_stage rust-surface-guard quick run_py scripts/quality/rust_surface_guard.py --mode enforce
    else
        SCOPE_SURFACE_ARGS=()
        for pkg in "${SCOPE_PKGS[@]}"; do
            SCOPE_SURFACE_ARGS+=(--root "crates/$pkg/src" --root "crates/$pkg/tests")
        done
        run_stage rust-surface-guard quick run_py scripts/quality/rust_surface_guard.py --mode enforce "${SCOPE_SURFACE_ARGS[@]}"
    fi
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
    if [[ "$scope" == "all" ]]; then
        run_stage test-layout-enforce quick run_py scripts/quality/test_layout_guard.py --mode enforce
    else
        SCOPE_CRATE_ARGS=()
        for pkg in "${SCOPE_PKGS[@]}"; do
            SCOPE_CRATE_ARGS+=(--crate "$pkg")
        done
        run_stage test-layout-enforce quick run_py scripts/quality/test_layout_guard.py --mode enforce "${SCOPE_CRATE_ARGS[@]}"
    fi
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
if maybe capture-isolation-self; then
    run_stage capture-isolation-self quick run_py scripts/test_capture_isolation.py --self-test --repo-root "$repo"
fi
if maybe capture-isolation; then
    capture_isolation_gate="${TM_CAPTURE_ISOLATION_GATE:-auto}"
    case "$capture_isolation_gate" in
    auto|0|1) ;;
    *)
        echo "TM_CAPTURE_ISOLATION_GATE must be auto, 0, or 1" >&2
        run_stage capture-isolation quick false
        ;;
    esac
    capture_isolation_host_ready=1
    if [[ "$capture_isolation_gate" == "auto" ]]; then
        [[ -n "${WAYLAND_DISPLAY:-}" && -n "${XDG_RUNTIME_DIR:-}" ]] || capture_isolation_host_ready=0
        if [[ "$capture_isolation_host_ready" == "1" && ! -S "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" ]]; then
            capture_isolation_host_ready=0
        fi
        for command in dbus-run-session busctl qdbus6 kwin_wayland niri; do
            if ! command -v "$command" >/dev/null 2>&1; then
                capture_isolation_host_ready=0
                break
            fi
        done
    fi
    if [[ "$capture_isolation_gate" == "1" || ( "$capture_isolation_gate" == "auto" && "$capture_isolation_host_ready" == "1" ) ]]; then
        run_stage capture-isolation quick timeout --kill-after=10s 240s python3 scripts/test_capture_isolation.py --repo-root "$repo"
    else
        reason="TM_CAPTURE_ISOLATION_GATE=$capture_isolation_gate"
        [[ "$capture_isolation_gate" == "auto" ]] && reason="$reason; no private Wayland/KWin host"
        echo "SKIP capture-isolation ($reason)"
        record capture-isolation quick SKIP 0
    fi
fi

[[ "$tier" == "quick" ]] && exit "$((failures > 0))"

# ---- standard --------------------------------------------------------
if maybe ui-route; then
    # This diff-only route is intentionally first: a UI change without the
    # required headless/capture mode should fail before compiling the workspace.
    if scope_skip ui-route "merge-owner diff routing; a scope runs its own interaction gate below" standard; then
        if [[ "$with_gui" == "1" ]]; then
            run_stage ui-route standard bash scripts/quality/ui-evidence-route.sh --with-gui
        else
            run_stage ui-route standard bash scripts/quality/ui-evidence-route.sh
        fi
    fi
fi
if [[ "$with_gui" == "1" ]]; then
    if maybe ui-capture-route; then
        # Capture acceptance is also cheap to reject early because it only
        # checks the freshness of receipts; the capture itself remains explicit.
        run_stage ui-capture-route standard bash scripts/quality/ui-evidence-route.sh --with-gui --require-capture
    fi
fi
if maybe deny; then
    run_stage deny standard timeout --kill-after=30s 600 cargo deny check
fi
if maybe clippy; then
    # `test-support` stays DEV-only: it enables gpui's upstream test-support
    # (which hard-wires X11) so headless GPUI tests compile; production
    # builds never enable it (strict Wayland product policy).
    if [[ "$scope" == "all" ]]; then
        run_stage clippy standard cargo clippy "${LOCK_ARGS[@]}" --workspace --all-targets --features test-support -- \
            -D warnings -W clippy::cognitive_complexity -W clippy::too_many_lines
    else
        SCOPE_CLIPPY_ARGS=()
        while read -r arg; do SCOPE_CLIPPY_ARGS+=("$arg"); done < <(scope_pkgs_args)
        SCOPE_FEATURE_ARGS=()
        if [[ "$scope" == "gpui" ]]; then
            SCOPE_FEATURE_ARGS+=(--features test-support)
        fi
        run_stage clippy standard cargo clippy "${LOCK_ARGS[@]}" "${SCOPE_CLIPPY_ARGS[@]}" --all-targets "${SCOPE_FEATURE_ARGS[@]}" -- \
            -D warnings -W clippy::cognitive_complexity -W clippy::too_many_lines
    fi
fi
if maybe nextest-core; then
    # Unit tests across every workspace crate plus non-root integration
    # targets (platform contract proofs, adapter tests, ...). The root
    # integration binaries are partitioned into their own layers below so a
    # failing layer is attributable without re-running the whole suite.
    # nextest 0.9 exposes `-j` and `--test-threads` as aliases for the same
    # option; pass it once so the gate remains valid across the installed
    # runner instead of triggering a duplicate-argument parse failure.
    if [[ "$scope" == "all" ]]; then
        run_stage nextest-core standard cargo nextest run "${LOCK_ARGS[@]}" --workspace --all-targets --features test-support -j 4 --profile ci -E 'not (binary(throughput) or binary(logic) or binary(gui) or binary(performance))'
    else
        SCOPE_NEXTEST_ARGS=()
        while read -r arg; do SCOPE_NEXTEST_ARGS+=("$arg"); done < <(scope_pkgs_args)
        SCOPE_FEATURE_ARGS=()
        if [[ "$scope" == "gpui" ]]; then
            SCOPE_FEATURE_ARGS+=(--features test-support)
        fi
        run_stage nextest-core standard cargo nextest run "${LOCK_ARGS[@]}" "${SCOPE_NEXTEST_ARGS[@]}" --all-targets "${SCOPE_FEATURE_ARGS[@]}" -j 4 --profile ci -E 'not (binary(throughput) or binary(logic) or binary(gui) or binary(performance))'
    fi
fi
if maybe nextest-logic; then
    if scope_skip nextest-logic "root acceptance layer" standard; then
        run_stage nextest-logic standard cargo nextest run "${LOCK_ARGS[@]}" -p taskmanager-gates --test logic -j 4 --profile ci
    fi
fi
if maybe nextest-gui; then
    if scope_skip nextest-gui "root acceptance layer" standard; then
        run_stage nextest-gui standard cargo nextest run "${LOCK_ARGS[@]}" -p taskmanager-gpui --test gui --features test-support -j 4 --profile ci
    fi
fi
if maybe nextest-perf; then
    if scope_skip nextest-perf "root acceptance layer" standard; then
        run_stage nextest-perf standard cargo nextest run "${LOCK_ARGS[@]}" -p taskmanager-gates --test performance -j 4 --profile ci
    fi
fi
if maybe live-smoke; then
    if scope_skip live-smoke "root acceptance layer" standard; then
        # One real-collector tick per supported platform (host-neutral invariants
        # only). Fixtures prove parsers; this stage proves the composition edge.
        run_stage live-smoke standard cargo nextest run "${LOCK_ARGS[@]}" -p taskmanager-gates --test logic -j 4 --profile ci -E 'test(live_smoke_)'
    fi
fi
if maybe doctests; then
    if [[ "$scope" == "all" ]]; then
        run_stage doctests standard cargo test "${LOCK_ARGS[@]}" --doc --workspace -j 4
    else
        SCOPE_DOC_ARGS=()
        while read -r arg; do SCOPE_DOC_ARGS+=("$arg"); done < <(scope_pkgs_args)
        run_stage doctests standard cargo test "${LOCK_ARGS[@]}" --doc "${SCOPE_DOC_ARGS[@]}" -j 4
    fi
fi
if maybe rustdoc; then
    if [[ "$scope" == "all" ]]; then
        run_stage rustdoc standard env RUSTDOCFLAGS="-D warnings" cargo doc "${LOCK_ARGS[@]}" --workspace --no-deps
    else
        SCOPE_RUSTDOC_ARGS=()
        while read -r arg; do SCOPE_RUSTDOC_ARGS+=("$arg"); done < <(scope_pkgs_args)
        run_stage rustdoc standard env RUSTDOCFLAGS="-D warnings" cargo doc "${LOCK_ARGS[@]}" "${SCOPE_RUSTDOC_ARGS[@]}" --no-deps
    fi
fi
if maybe nvidia-fallback; then
    if scope_skip nvidia-fallback "product fallback build" standard; then
        # ADR-051: the vendor matrix runs against the GPUI product only.
        run_stage nvidia-fallback standard cargo nextest run "${LOCK_ARGS[@]}" -p taskmanager-gpui --lib --features hardware-all,nvidia -j 4 --profile ci
    fi
fi
if maybe shape-tui; then
    if scope_skip shape-tui "product fallback build" standard; then
        run_stage shape-tui standard bash -c \
            'cargo check ${TM_CARGO_LOCK---locked} -p taskmanager-tui --all-targets --no-default-features && cargo nextest run ${TM_CARGO_LOCK---locked} -p taskmanager-tui --all-targets --no-default-features -j 4 --profile ci -E "not binary(throughput)"'
    fi
fi
if maybe shape-iced; then
    if scope_skip shape-iced "product fallback build" standard; then
        run_stage shape-iced standard bash -c \
            'cargo check ${TM_CARGO_LOCK---locked} -p taskmanager-iced --all-targets --no-default-features && cargo nextest run ${TM_CARGO_LOCK---locked} -p taskmanager-iced --all-targets --no-default-features -j 4 --profile ci -E "not binary(throughput)"'
    fi
fi
if [[ "$skip_release" == "1" ]]; then
    record release standard SKIP 0
elif scope_skip release "merge-owner release smoke" standard; then
    if maybe release; then
        # Reuse the same release/package smoke as the blocking Linux CI job.
        # PR-style LTO/codegen overrides keep this local gate fast; extended
        # bloat measurement rebuilds with the shipping profile below.
        run_stage release standard env PR_SMOKE_PROFILE=true scripts/quality/release-smoke.sh
    fi
fi
if [[ "$with_gui" == "1" ]]; then
    if maybe gpui-interactions; then
        run_stage gpui-interactions standard timeout --kill-after=10s 2400 bash scripts/accept-gpui-interactions.sh
    fi
    if maybe bevy-interactions; then
        # Fourth frontend's headless interaction matrix: same contract as the
        # gpui stage — the gate discovers every named test in the lib target,
        # then runs the complete target under the locked workspace.
        run_stage bevy-interactions standard timeout --kill-after=10s 1200 bash scripts/accept-bevy-interactions.sh
    fi
elif [[ "$scope" == "bevy" ]]; then
    # The Bevy matrix is headless: a scoped Bevy line runs it on every
    # standard pass, not only --with-gui.
    if maybe bevy-interactions; then
        run_stage bevy-interactions standard timeout --kill-after=10s 1200 bash scripts/accept-bevy-interactions.sh
    fi
fi

[[ "$tier" == "standard" ]] && exit "$((failures > 0))"

# ---- extended --------------------------------------------------------
if maybe coverage; then
    if scope_skip coverage "workspace-wide measurement" extended; then
        run_stage coverage extended bash -c \
            'cargo llvm-cov nextest ${TM_CARGO_LOCK---locked} --workspace --all-targets --features test-support --profile ci -j 4 -E "not binary(throughput)" --lcov --output-path target/lcov.info --fail-under-lines 71 && timeout --kill-after=10s 120 python3 scripts/quality/per_crate_coverage_gate.py --lcov target/lcov.info --check'
    fi
fi
if maybe mutants; then
    if scope_skip mutants "workspace-wide measurement" extended; then
        # Decision logic in core/application plus the Linux fixture parsers: all
        # three are the layers whose silent breakage the behavior gates must catch.
        run_stage mutants extended scripts/quality/mutants-in-diff.sh --packages taskmanager-core,taskmanager-application,taskmanager-platform-linux --min-score 80
    fi
fi
if maybe miri; then
    if scope_skip miri "workspace-wide measurement" extended; then
        run_stage miri extended scripts/quality/miri-boundaries.sh
    fi
fi
if maybe fuzz-build; then
    if scope_skip fuzz-build "workspace-wide measurement" extended; then
        # cargo-fuzz 0.13.x rejects --manifest-path: each fuzz workspace is built
        # from its own directory (they are deliberately not workspace members).
        run_stage fuzz-build extended bash -c \
            'cd crates/taskmanager-afpacket/fuzz && cargo +nightly fuzz build && cd ../../taskmanager-platform-linux/fuzz && cargo +nightly fuzz build && cd ../../../crates/taskmanager-fd-bridge/fuzz && cargo +nightly fuzz build'
    fi
fi
if [[ "$with_fuzz_runs" == "1" ]]; then
    if maybe fuzz-run; then
        if scope_skip fuzz-run "workspace-wide measurement" extended; then
            run_stage fuzz-run extended bash -c \
                'cd crates/taskmanager-afpacket/fuzz && cargo +nightly fuzz run five_tuple -- -runs=2000000 -timeout=5 && cd ../../taskmanager-platform-linux/fuzz && cargo +nightly fuzz run proc_parsers -- -runs=2000000 -timeout=5 && cargo +nightly fuzz run mm_stat -- -runs=2000000 -timeout=5 && cd ../../../crates/taskmanager-fd-bridge/fuzz && cargo +nightly fuzz run scm_rights_walk -- -runs=2000000 -timeout=5'
        fi
    fi
fi
if maybe bloat; then
    if scope_skip bloat "workspace-wide measurement" extended; then
        run_stage bloat extended bash -c \
            'CARGO_PROFILE_RELEASE_LTO=thin CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 CARGO_PROFILE_RELEASE_STRIP=debuginfo timeout --kill-after=30s 3600 cargo build ${TM_CARGO_LOCK---locked} --release -j 4 -p taskmanager-gpui; scripts/quality/trend-gate.sh --metric bloat --current "$(stat -c %s target/release/taskforest-g 2>/dev/null || echo 0)" --trend docs/quality/bloat-trend.tsv --limit 5'
    fi
fi
if maybe benches; then
    if scope_skip benches "workspace-wide measurement" extended; then
        run_stage benches extended scripts/quality/bench-gate.sh
    fi
fi

exit "$((failures > 0))"
