#!/usr/bin/env bash
# local-gates.sh (Windows) — the Windows-native tier of the local gate set.
#
# This is the Git Bash counterpart of scripts/quality/local-gates.sh, which
# targets the Linux workstation (Wayland capture, taskset pinning, Miri over
# Linux-only boundary crates). Stage inheritance is deliberate:
#
#   quick      identical policy surface: fmt + the pure-Python governance
#              gates. The install-manager smoke is SKIPPED (it stages a
#              usr/lib + polkit-1 tree, a Linux install layout).
#   standard   clippy, the nextest workspace pass split into core/logic/gui/
#              perf layers (integration groups compile on Windows since the
#              platform-native composition edge replaced the direct OS-crate
#              imports in the test targets; `--only nextest-core` gives a
#              bottom-up dev loop), doctests, rustdoc, the three shape checks
#              (check-only, the same scope as the portability Windows job),
#              release build, and — with --with-gui — the headless GPUI
#              interaction matrix (scripts/windows/accept-gpui-interactions.sh
#              keeps Windows evidence separate and needs no compositor).
#              `cargo deny` is skipped when the tool or its advisory database
#              is unavailable (probed, never assumed).
#   extended   rejected: Miri/fuzz/mutants/coverage/bloat stay on the Linux
#              workstation pass where their boundary crates and toolchains
#              exist.
#
# Every stage is bounded by `timeout --kill-after=`; scratch lives under
# .tmp/ on the repository drive and is removed on exit; TMP/TEMP/TMPDIR are
# all redirected because MSVC tooling ignores TMPDIR. Parallelism caps at 4
# jobs (repo policy; .config/nextest.toml pins test threads).
#
# Usage:
#   bash scripts/windows/local-gates.sh [quick|standard] [--with-gui]
#     [--skip-release] [--only <stage>]
#   No tier argument runs `quick`. Run scripts/windows/env-probe.sh first on
#   a fresh machine.
#
# Environment overrides:
#   JOBS=<n>                cargo parallelism (default 4)
#   STAGE_TIMEOUT=<sec>     per-stage deadline (default 3600)
#   QUICK_TIMEOUT=<sec>     quick-stage deadline (default 300)

set -u

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"

kernel="$(uname -s)"
case "$kernel" in
MINGW* | MSYS* | CYGWIN*) ;;
*)
    echo "this gate entry is Windows-only (uname: $kernel); on Linux use scripts/quality/local-gates.sh" >&2
    exit 2
    ;;
esac

export CARGO_BUILD_JOBS="${JOBS:-4}"
export TMPDIR="$repo/.tmp/local-gates-win-$$-$(date +%s)/tmp"
export TMP="$TMPDIR"
export TEMP="$TMPDIR"
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

skip_stage() {
    local name="$1"
    local tier="$2"
    local reason="$3"
    echo "SKIP $name ($reason)"
    record "$name" "$tier" "SKIP" 0
    stages_run=$((stages_run + 1))
}

only="${ONLY_STAGE:-}"

maybe() {
    if [[ -n "$only" && "$only" != "$1" ]]; then
        skip_stage "$1" "quick" "--only $only"
        return 1
    fi
    return 0
}

with_gui=0
skip_release=0
tier="quick"
while [[ $# -gt 0 ]]; do
    case "$1" in
    quick | standard) tier="$1" ;;
    extended)
        echo "the extended tier is Linux-workstation-only (Miri/fuzz/mutants/coverage);" >&2
        echo "run scripts/quality/local-gates.sh extended there" >&2
        exit 2
        ;;
    --with-gui) with_gui=1 ;;
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
    echo "=== windows local-gates summary ($tier, $stages_run stages, $failures failed) ==="
    if [[ -f "$results_file" ]]; then
        column -t -s $'\t' "$results_file" 2>/dev/null || cat "$results_file"
    fi
    if [[ $failures -gt 0 ]]; then
        echo "windows local-gates: $failures stage(s) failed (took $(( $(date +%s) - started_at ))s)"
    else
        echo "windows local-gates: clean (took $(( $(date +%s) - started_at ))s)"
    fi
    rm -rf "$scratch"
    exit "$rc"
}
trap cleanup EXIT INT TERM

# Hard requirements (fail fast with a remedy instead of a stage-wall of noise).
for command in cargo rustc git sha256sum timeout column; do
    if ! command -v "$command" >/dev/null 2>&1; then
        echo "required command is unavailable: $command (run scripts/windows/env-probe.sh)" >&2
        exit 2
    fi
done
if ! timeout 5s python3 --version >/dev/null 2>&1; then
    echo "Python 3 interpreter is unavailable (run scripts/windows/env-probe.sh)" >&2
    exit 2
fi

# The Linux and macOS adapters are single-platform artifacts: platform-linux's
# nix/Unix call sites (errno/unistd/ifaddrs) and platform-macos's sysinfo uid
# handling (a Windows SID on this target) are not cfg-gated for Windows, and
# the Windows product never links either (taskmanager-platform-native selects
# platform-windows). Workspace-wide stages therefore exclude both here,
# mirroring how Linux gates never compile platform-windows's Windows-only
# seams. The audited boundary crates themselves DO compile on every target
# (crate-root Linux gating makes them empty elsewhere).
exclude_platform_adapters="--exclude taskmanager-platform-linux --exclude taskmanager-platform-macos"
if ! cargo nextest --version >/dev/null 2>&1; then
    echo "cargo-nextest is unavailable; bare cargo test is banned in this repository" >&2
    exit 2
fi

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
    # The smoke test stages a usr/lib + polkit-1 install tree: Linux layout.
    skip_stage install-manager-smoke quick "Linux-only install layout"
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
if maybe test-layout-self; then
    run_stage test-layout-self quick run_py scripts/quality/test_layout_guard.py --self-test
fi
if maybe test-layout-enforce; then
    run_stage test-layout-enforce quick run_py scripts/quality/test_layout_guard.py --mode enforce
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

[[ "$tier" == "quick" ]] && exit "$((failures > 0))"

# ---- standard --------------------------------------------------------
if maybe deny; then
    if cargo deny --version >/dev/null 2>&1; then
        run_stage deny standard timeout --kill-after=30s 600 cargo deny check
    else
        skip_stage deny standard "cargo-deny not installed"
    fi
fi
if maybe clippy; then
    run_stage clippy standard cargo clippy --locked --workspace --all-targets --features test-support -j 4 $exclude_platform_adapters -- -D warnings
fi
if maybe nextest-core; then
    # Unit tests across every workspace crate plus non-root integration
    # targets, excluding the single-platform Linux/macOS adapters. The three
    # root integration binaries are partitioned into their own layers below.
    # nextest 0.9 exposes `-j` and `--test-threads` as aliases of the same
    # option; pass it once (repo contract, .config/nextest.toml).
    run_stage nextest-core standard cargo nextest run --locked --workspace --all-targets --features test-support -j 4 --profile ci $exclude_platform_adapters -E 'not (binary(throughput) or binary(logic) or binary(gui) or binary(performance))'
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
    # One real-collector tick on the Windows host (host-neutral invariants
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
    run_stage doctests standard cargo test --locked --doc --workspace -j 4 $exclude_platform_adapters
fi
if maybe rustdoc; then
    run_stage rustdoc standard env RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps -j 4 $exclude_platform_adapters
fi
if maybe shape-tui; then
    # Shape checks are compile-scope on Windows, mirroring the portability job.
    run_stage shape-tui standard cargo check --locked --workspace --all-targets --no-default-features --features hardware-all,ui-tui -j 4 $exclude_platform_adapters
fi
if maybe shape-iced; then
    run_stage shape-iced standard cargo check --locked --workspace --all-targets --no-default-features --features hardware-all,ui-iced -j 4 $exclude_platform_adapters
fi
if maybe ui-gpui-minimal; then
    run_stage ui-gpui-minimal standard cargo check --locked --no-default-features --features ui-gpui -j 4
fi
if [[ "$skip_release" == "1" ]]; then
    skip_stage release standard "--skip-release"
else
    if maybe release; then
        run_stage release standard cargo build --locked --release -j 4
    fi
fi
if [[ "$with_gui" == "1" ]]; then
    if maybe ui-capture-route; then
        # Capture acceptance additionally requires fresh pixel receipts for
        # every touched frontend shape (run the selected capture workflow).
        run_stage ui-capture-route standard bash scripts/quality/ui-evidence-route.sh --with-gui --require-capture
    fi
    if maybe gpui-interactions; then
        run_stage gpui-interactions standard timeout --kill-after=10s 2400 bash scripts/windows/accept-gpui-interactions.sh
    fi
fi

exit "$((failures > 0))"
