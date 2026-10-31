#!/usr/bin/env bash
# semver-api-gate.sh — public-API compatibility gate for the publishable crates.
#
# Why this exists: the workspace publishes a set of library crates whose
# public API is a promise to downstream consumers. A stage that only prints
# "SKIP" when the tool is missing is not a gate — it is a comment. This
# script actually runs `cargo semver-checks` and fails when a publishable
# crate breaks its public API against a baseline.
#
# Baseline: the publishable crates are not on crates.io yet, so there is no
# published version to compare against. The gate therefore uses a git
# revision as the baseline (`--baseline-rev`), taken from
# `SEMVER_BASELINE_REV` (default: `origin/main`). The revision must resolve
# in the local clone; fetch the missing history first (`git fetch origin`).
#
# Local triage: `SEMVER_ONLY="taskforest-core taskforest-ipc"` restricts the
# run to the named packages. It exists so a maintainer can reproduce a single
# failure without rebuilding all publishable crates; CI never sets it.
#
# Discovery: the checked set is derived from the manifests, never hand-listed.
# Every `crates/*/Cargo.toml` whose `[package]` section does not say
# `publish = false` is publishable and therefore checked; the workspace root
# manifest is scanned the same way (the root package is `publish = false`, so
# it drops out on its own). A crate without a library target has no public
# API and is skipped. No package name is hard-coded here, so a workspace-wide
# rename needs no change to this script.
#
# New crates: a crate that does not exist in the baseline revision has no
# earlier public API to compare against, so it is reported as SKIP instead of
# failing the gate. Such a crate is a fresh publication, and its first
# release sets the API contract.
#
# Vacuity: a run that compares NOTHING is not a green run — it is a gate that
# proved nothing. If every publishable crate is missing from the baseline (the
# expected state right after a package rename, where the baseline still
# carries the old names), the gate fails instead of reporting success.
#
# Usage:
#   scripts/quality/semver-api-gate.sh
#
# Environment:
#   SEMVER_BASELINE_REV             git revision used as the API baseline
#                                   (default: origin/main)
#   SEMVER_ALLOW_MISSING_BASELINE   when 1, an unresolvable baseline downgrades
#                                   the gate to a SKIP instead of a failure
#   SEMVER_IGNORE_FILE              override for the ignore-list path
#                                   (default: scripts/quality/semver-ignore-lints.txt)
#   SEMVER_CRATE_TIMEOUT            per-crate deadline in seconds (default 900)
#   SEMVER_RUSTDOC_JSON             auto (default) | 1 | 0 — generate both sides
#                                   with `cargo rustdoc --locked` and hand the two
#                                   JSON files to the tool, so the baseline side is
#                                   pinned by its own tree lockfile. `auto` uses the
#                                   flow only when the tool accepts prebuilt rustdoc
#                                   JSON and the nightly toolchain is installed
#   SEMVER_RUSTDOC_TOOLCHAIN        toolchain that can emit rustdoc JSON
#                                   (default: nightly; rustdoc JSON is unstable)
#
# Exit codes:
#   0  every checked crate kept its public API
#   1  a crate broke its public API, or the gate could not be carried out
#   2  the gate is not available: cargo-semver-checks is not installed, or the
#      baseline does not resolve and SEMVER_ALLOW_MISSING_BASELINE=1 was set
#
# The tool's own exit codes are mapped like this: 0 = clean, 100 = deny-level
# semver violations found, 101 = the check could not complete. Every other
# code (including a timeout) is a failure.
#
# The tool's command line is probed before use (`--help`), because the
# available subcommand and flags differ between cargo-semver-checks releases:
# the check subcommand is named `check-release` in the current release, and
# `--ignore` / `--locked` are not accepted by every release. An unsupported
# flag is reported explicitly instead of being passed blindly.

set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"

crate_timeout="${SEMVER_CRATE_TIMEOUT:-900}"
ignore_file="${SEMVER_IGNORE_FILE:-$repo/scripts/quality/semver-ignore-lints.txt}"

log() { printf 'semver-gate: %s\n' "$*"; }
fail() { printf 'semver-gate: %s\n' "$*" >&2; }

# ---------------------------------------------------------------------------
# Tool probe — a missing tool is "not available", not "failed".
# ---------------------------------------------------------------------------
if ! command -v cargo >/dev/null 2>&1; then
    fail "cargo is not on PATH; the semver gate cannot run."
    exit 2
fi
if ! command -v cargo-semver-checks >/dev/null 2>&1; then
    fail "cargo-semver-checks is not installed."
    fail "install it with: cargo install cargo-semver-checks --locked"
    fail "CI installs the pinned release with taiki-e/install-action instead."
    exit 2
fi

# ---------------------------------------------------------------------------
# Manifest reading
# ---------------------------------------------------------------------------
read_manifest() {
    # Reads a Cargo manifest on stdin and prints "<name>\t<publishable:0|1>".
    # Only the `[package]` section is considered: `publish = false` there is
    # the one spelling that keeps a crate out of the published set.
    awk '
        BEGIN { in_package = 0; name = ""; publishable = 1 }
        /^\[package\][ \t]*$/ { in_package = 1; next }
        /^\[/ { in_package = 0 }
        in_package {
            line = $0
            sub(/[ \t]*#.*$/, "", line)
            if (line ~ /^name[ \t]*=/) {
                value = line
                sub(/^name[ \t]*=[ \t]*/, "", value)
                gsub(/"/, "", value)
                gsub(/^[ \t]+|[ \t]+$/, "", value)
                name = value
            } else if (line ~ /^publish[ \t]*=/) {
                value = line
                sub(/^publish[ \t]*=[ \t]*/, "", value)
                gsub(/[ \t]/, "", value)
                if (value == "false") publishable = 0
            }
        }
        END { if (name != "") print name "\t" publishable }
    '
}

manifest_has_lib_target() {
    # $1: manifest path. A crate is checkable when it declares a `[lib]`
    # section or ships the conventional `src/lib.rs`.
    local manifest="$1"
    local dir
    dir="$(dirname "$manifest")"
    if [[ -f "$dir/src/lib.rs" ]]; then
        return 0
    fi
    if grep -qE '^\[lib\][ \t]*$' "$manifest"; then
        return 0
    fi
    return 1
}

# ---------------------------------------------------------------------------
# Crate discovery
# ---------------------------------------------------------------------------
publishable_crates=()
skipped_crates=()

for manifest in "$repo/Cargo.toml" "$repo"/crates/*/Cargo.toml; do
    [[ -f "$manifest" ]] || continue
    info="$(read_manifest <"$manifest")"
    [[ -n "$info" ]] || continue
    crate_name="${info%%$'\t'*}"
    crate_publishable="${info##*$'\t'}"
    if [[ "$crate_publishable" != "1" ]]; then
        skipped_crates+=("$crate_name (publish = false)")
        continue
    fi
    if ! manifest_has_lib_target "$manifest"; then
        skipped_crates+=("$crate_name (no library target)")
        continue
    fi
    publishable_crates+=("$crate_name")
done

if [[ "${#publishable_crates[@]}" -eq 0 ]]; then
    fail "no publishable crate was discovered; the gate cannot verify anything."
    fail "check the crate layout and the 'publish = false' markers before trusting a green run."
    exit 1
fi

if [[ -n "${SEMVER_ONLY:-}" ]]; then
    log "SEMVER_ONLY is set; restricting the run to: $SEMVER_ONLY"
    selected=()
    for crate_name in ${SEMVER_ONLY//,/ }; do
        found=0
        for known in "${publishable_crates[@]}"; do
            [[ "$known" == "$crate_name" ]] && found=1
        done
        if [[ "$found" -eq 0 ]]; then
            fail "SEMVER_ONLY names '$crate_name', which is not a discovered publishable crate."
            exit 1
        fi
        selected+=("$crate_name")
    done
    publishable_crates=("${selected[@]}")
fi

log "publishable crates (${#publishable_crates[@]}):"
for crate_name in "${publishable_crates[@]}"; do
    log "  - $crate_name"
done
if [[ "${#skipped_crates[@]}" -gt 0 ]]; then
    log "not published (skipped): ${skipped_crates[*]}"
fi

# ---------------------------------------------------------------------------
# Baseline resolution
# ---------------------------------------------------------------------------
baseline_rev="${SEMVER_BASELINE_REV:-origin/main}"
baseline_sha=""
if baseline_sha="$(git -C "$repo" rev-parse --verify --quiet "${baseline_rev}^{commit}" 2>/dev/null)"; then
    :
else
    baseline_sha=""
fi
if [[ -z "$baseline_sha" ]]; then
    if [[ "${SEMVER_ALLOW_MISSING_BASELINE:-0}" == "1" ]]; then
        log "SKIP: baseline revision '$baseline_rev' does not resolve in this checkout."
        log "      fetch the missing history (git fetch origin) and re-run to check the API."
        exit 2
    fi
    fail "baseline revision '$baseline_rev' does not resolve in this repository."
    fail "the publishable crates are not published on crates.io, so the gate needs a git baseline."
    fail "fetch the missing history (git fetch origin) or point SEMVER_BASELINE_REV at a"
    fail "resolvable revision (a commit sha, a tag, or a branch)."
    exit 1
fi
log "baseline: $baseline_rev ($baseline_sha)"

# Package names that exist in the baseline tree. A crate missing from this
# set did not exist at the baseline revision and therefore has no earlier
# public API to compare against.
baseline_names=()
while IFS= read -r path; do
    [[ -n "$path" ]] || continue
    baseline_name="$(git -C "$repo" show "${baseline_sha}:${path}" 2>/dev/null | read_manifest || true)"
    [[ -n "$baseline_name" ]] || continue
    baseline_names+=("${baseline_name%%$'\t'*}")
done < <(git -C "$repo" ls-tree -r --name-only "$baseline_sha" 2>/dev/null |
    grep -E '^crates/[^/]+/Cargo\.toml$' || true)
baseline_root_name="$(git -C "$repo" show "${baseline_sha}:Cargo.toml" 2>/dev/null | read_manifest || true)"
if [[ -n "$baseline_root_name" ]]; then
    baseline_names+=("${baseline_root_name%%$'\t'*}")
fi

baseline_has_crate() {
    local candidate="$1"
    local known
    for known in "${baseline_names[@]}"; do
        if [[ "$known" == "$candidate" ]]; then
            return 0
        fi
    done
    return 1
}

# ---------------------------------------------------------------------------
# Ignore list
# ---------------------------------------------------------------------------
ignore_lints=()
if [[ -f "$ignore_file" ]]; then
    while IFS= read -r line || [[ -n "$line" ]]; do
        line="${line%%#*}"
        line="$(printf '%s' "$line" | tr -d '[:space:]')"
        if [[ -n "$line" ]]; then
            ignore_lints+=("$line")
        fi
    done <"$ignore_file"
else
    fail "the ignore list '$ignore_file' is missing."
    fail "create it (an empty file means: ignore nothing) before running the gate."
    exit 1
fi

# ---------------------------------------------------------------------------
# Command-line probe — the tool's subcommand and flags differ per release.
# ---------------------------------------------------------------------------
help_text="$(cargo semver-checks --help 2>&1 || true)"
if [[ -z "$help_text" ]]; then
    fail "cannot read the output of 'cargo semver-checks --help'; the tool is unusable."
    exit 1
fi

subcommand=""
for candidate in check-release check diff-files; do
    candidate_help="$(cargo semver-checks "$candidate" --help 2>&1 || true)"
    if [[ -n "$candidate_help" ]] && grep -q -- '--baseline-rev' <<<"$candidate_help"; then
        subcommand="$candidate"
        break
    fi
done
if [[ -z "$subcommand" ]]; then
    if grep -q -- '--baseline-rev' <<<"$help_text" && grep -q -- '--package' <<<"$help_text"; then
        log "using the top-level command form (no check subcommand)"
    else
        fail "the installed cargo-semver-checks exposes no usable check command."
        fail "expected a 'check'/'check-release' subcommand, or top-level --baseline-rev and"
        fail "--package flags. Run 'cargo semver-checks --help' to see what this build offers."
        exit 1
    fi
else
    log "check subcommand: $subcommand"
fi

combined_help="$help_text"
if [[ -n "$subcommand" ]]; then
    combined_help="$combined_help"$'\n'"$(cargo semver-checks "$subcommand" --help 2>&1 || true)"
fi

supports_locked=0
if grep -qE '^[[:space:]]+--locked([[:space:]]|,|$)' <<<"$combined_help"; then
    supports_locked=1
fi
supports_ignore=0
if grep -qE '^[[:space:]]+--ignore([[:space:]]|,|$)' <<<"$combined_help"; then
    supports_ignore=1
fi

# ---------------------------------------------------------------------------
# Lockfile-pinned rustdoc-JSON flow
# ---------------------------------------------------------------------------
# The tool resolves the baseline side in a placeholder workspace of its own, so
# the repository lockfile does not pin that side. Where the tool accepts
# prebuilt rustdoc JSON, the gate can generate BOTH sides itself with
# `cargo rustdoc --locked` and hand over the two files: each side then comes
# from a tree whose own lockfile pins it, and the tool only compares the JSON.
# Emitting rustdoc JSON is an unstable rustdoc feature, so the flow needs a
# nightly toolchain and stays optional.
supports_current_rustdoc=0
supports_baseline_rustdoc=0
if grep -qE '^[[:space:]]+--current-rustdoc([[:space:]]|,|$)' <<<"$combined_help"; then
    supports_current_rustdoc=1
fi
if grep -qE '^[[:space:]]+--baseline-rustdoc([[:space:]]|,|$)' <<<"$combined_help"; then
    supports_baseline_rustdoc=1
fi
rustdoc_toolchain="${SEMVER_RUSTDOC_TOOLCHAIN:-nightly}"
rustdoc_json=0
case "${SEMVER_RUSTDOC_JSON:-auto}" in
    0) ;;
    1)
        if [[ "$supports_current_rustdoc" -eq 1 && "$supports_baseline_rustdoc" -eq 1 ]] &&
            rustup run "$rustdoc_toolchain" rustdoc -vV >/dev/null 2>&1 &&
            command -v python3 >/dev/null 2>&1; then
            rustdoc_json=1
        else
            fail "SEMVER_RUSTDOC_JSON=1 was requested but the lockfile-pinned flow is unavailable:"
            fail "  the tool must accept --current-rustdoc/--baseline-rustdoc, the"
            fail "  '$rustdoc_toolchain' toolchain must be installed, and 'command -v python3' must succeed."
            exit 1
        fi
        ;;
    auto)
        if [[ "$supports_current_rustdoc" -eq 1 && "$supports_baseline_rustdoc" -eq 1 ]] &&
            rustup run "$rustdoc_toolchain" rustdoc -vV >/dev/null 2>&1 &&
            command -v python3 >/dev/null 2>&1; then
            rustdoc_json=1
        fi
        ;;
    *)
        fail "SEMVER_RUSTDOC_JSON must be auto, 0, or 1"
        exit 1
        ;;
esac

ignore_arg=""
if [[ "${#ignore_lints[@]}" -gt 0 ]]; then
    if [[ "$supports_ignore" -eq 1 ]]; then
        for lint_name in "${ignore_lints[@]}"; do
            if [[ -z "$ignore_arg" ]]; then
                ignore_arg="$lint_name"
            else
                ignore_arg="$ignore_arg,$lint_name"
            fi
        done
        log "ignored lints: $ignore_arg"
    else
        fail "the ignore list names ${#ignore_lints[@]} lint(s), but the installed"
        fail "cargo-semver-checks does not accept an --ignore flag."
        fail "Downgrade the lint in the manifest instead (the tool-independent way):"
        fail "  [package.metadata.cargo-semver-checks.lints]"
        for lint_name in "${ignore_lints[@]}"; do
            fail "  $lint_name = \"allow\""
        done
        exit 1
    fi
fi

if [[ "$supports_locked" -eq 1 ]]; then
    log "the tool accepts --locked; the check runs against the pinned lockfile"
elif [[ "$rustdoc_json" -eq 1 ]]; then
    log "no --locked flag in this release; both sides are generated here with"
    log "  'cargo rustdoc --locked' on $rustdoc_toolchain, so each side is pinned by its"
    log "  own tree lockfile and the tool only compares the two rustdoc JSON files."
else
    log "note: this cargo-semver-checks release has no --locked flag; it resolves the"
    log "      checked crates in its own placeholder workspace, so the repository"
    log "      lockfile does not pin the scan. The lock policy stays with the build gates."
fi

# ---------------------------------------------------------------------------
# Per-crate checks
# ---------------------------------------------------------------------------
log_dir="$(mktemp -d "${TMPDIR:-/tmp}/semver-api-gate.XXXXXX")"
cleanup() { rm -rf "$log_dir"; }
trap cleanup EXIT

# cargo-semver-checks keeps a small working cache under XDG_CACHE_HOME. The
# lockfile-pinned flow needs a writable one; when the ambient cache is missing
# or read-only (sandboxed runs, read-only CI caches) point the tool at the
# run's own temporary directory instead of dying on a cache-directory error.
if [[ "$rustdoc_json" -eq 1 ]]; then
    cache_home="${XDG_CACHE_HOME:-${HOME:-/nonexistent}/.cache}"
    if [[ ! -d "$cache_home" || ! -w "$cache_home" ]]; then
        export XDG_CACHE_HOME="$log_dir/cache"
        mkdir -p "$XDG_CACHE_HOME"
        log "pointing the tool cache at the run directory (ambient cache is not writable)"
    fi
fi

summarize_failures() {
    # First few findings of a failed check, one line per lint.
    local logfile="$1"
    local lints=""
    local line
    while IFS= read -r line; do
        [[ -n "$line" ]] || continue
        if [[ -z "$lints" ]]; then
            lints="$line"
        else
            lints="$lints, $line"
        fi
    done < <(grep -E '^--- (error|warning) [A-Za-z0-9_]+:' "$logfile" 2>/dev/null |
        sed -E 's/^--- (error|warning) ([A-Za-z0-9_]+):.*/\1:\2/' | sort -u || true)
    printf '%s' "$lints"
}

first_error_line() {
    # The first diagnostic line of a failed check.
    local logfile="$1"
    local line
    line="$(grep -m1 -iE '^[[:space:]]*error' "$logfile" 2>/dev/null || true)"
    if [[ -z "$line" ]]; then
        line="$(grep -m1 -vE '^[[:space:]]*$' "$logfile" 2>/dev/null || true)"
    fi
    printf '%s' "${line:0:200}"
}

# The library target name (underscored) of one package inside one tree. The
# rustdoc-JSON artifact is named after the target, never after the package, and
# the two differ whenever a crate's package and lib names diverge.
cargo_lib_target_name() {
    local root="$1" package="$2"
    (cd "$root" && cargo metadata --no-deps --locked --format-version 1 2>/dev/null) |
        SEMVER_TARGET_PACKAGE="$package" timeout 60s python3 -c '
import json, os, sys

package = os.environ["SEMVER_TARGET_PACKAGE"]
try:
    metadata = json.load(sys.stdin)
except Exception:
    sys.exit(1)
for candidate in metadata.get("packages", []):
    if candidate.get("name") != package:
        continue
    for target in candidate.get("targets", []):
        if "lib" in target.get("kind", []):
            print(target["name"].replace("-", "_"))
            sys.exit(0)
sys.exit(1)
'
}

# Emit rustdoc JSON for one package in one tree, with that tree's lockfile
# pinning the dependency graph. `build.warnings = "deny"` is relaxed for the
# doc build only: this gate compares API shapes, and a doc lint is not an API
# break. Returns non-zero so the caller can fall back to the tool-driven path.
rustdoc_json_for() {
    local root="$1" package="$2" output="$3" target="$4"
    local lib_name
    lib_name="$(cargo_lib_target_name "$root" "$package")" || return 1
    [[ -n "$lib_name" ]] || return 1
    if ! (cd "$root" && timeout --kill-after=30s "$crate_timeout" \
        env RUSTUP_TOOLCHAIN="$rustdoc_toolchain" \
        cargo rustdoc -p "$package" --lib --locked \
        --config 'build.warnings="warn"' \
        -Z unstable-options --output-format json \
        --target-dir "$target" >/dev/null 2>&1); then
        return 1
    fi
    local produced="$target/doc/$lib_name.json"
    [[ -f "$produced" ]] || return 1
    cp "$produced" "$output"
}

results=()
failures=0
compared=0

# Baseline side of the lockfile-pinned flow: the exact baseline tree, checked
# out beside the current one, so its own Cargo.lock pins its own build.
baseline_tree=""
rustdoc_target="${CARGO_TARGET_DIR:-$repo/target}"
if [[ "$rustdoc_json" -eq 1 ]]; then
    baseline_tree="$log_dir/baseline-tree"
    mkdir -p "$baseline_tree"
    if git -C "$repo" archive "$baseline_sha" | tar -x -C "$baseline_tree"; then
        log "materialized the baseline tree for the lockfile-pinned flow"
    else
        log "note: the baseline tree could not be materialized; falling back to the"
        log "      tool-driven baseline (the baseline side is then not lockfile-pinned)."
        rustdoc_json=0
        baseline_tree=""
    fi
fi

for crate_name in "${publishable_crates[@]}"; do
    if ! baseline_has_crate "$crate_name"; then
        log "skipping $crate_name: not present in the baseline revision (new crate, no earlier API)"
        results+=("$crate_name"$'\t'"SKIP"$'\t'"not present in the baseline revision")
        continue
    fi

    # Preferred: build both sides here with `cargo rustdoc --locked` and hand
    # the two JSON files over, so the baseline side is pinned by the baseline
    # tree's own lockfile. Fall back to `--baseline-rev` (whose baseline side
    # the tool resolves itself, unpinned) when either side cannot be produced,
    # or when the tool cannot read the JSON. Either way the run is logged.
    used_rustdoc=0
    if [[ "$rustdoc_json" -eq 1 ]]; then
        cur_json="$log_dir/$crate_name.current-rustdoc.json"
        base_json="$log_dir/$crate_name.baseline-rustdoc.json"
        if rustdoc_json_for "$repo" "$crate_name" "$cur_json" "$rustdoc_target" &&
            rustdoc_json_for "$baseline_tree" "$crate_name" "$base_json" "$rustdoc_target"; then
            used_rustdoc=1
        else
            log "  $crate_name: rustdoc JSON could not be produced; using the tool-driven baseline"
        fi
    fi

    if [[ "$used_rustdoc" -eq 1 ]]; then
        cmd=(cargo semver-checks)
        if [[ -n "$subcommand" ]]; then
            cmd+=("$subcommand")
        fi
        cmd+=(--manifest-path "$repo/Cargo.toml" --package "$crate_name"
            --current-rustdoc "$cur_json" --baseline-rustdoc "$base_json")
        if [[ -n "$ignore_arg" ]]; then
            cmd+=("--ignore" "$ignore_arg")
        fi
        log "checking $crate_name ... (lockfile-pinned rustdoc JSON)"
        rc=0
        timeout --kill-after=30s "$crate_timeout" "${cmd[@]}" >"$log_dir/$crate_name.log" 2>&1 || rc=$?
        if [[ "$rc" -eq 101 ]]; then
            log "  $crate_name: the rustdoc JSON comparison could not complete; retrying against the tool-driven baseline"
            used_rustdoc=0
        fi
    fi

    if [[ "$used_rustdoc" -eq 0 ]]; then
        cmd=(cargo semver-checks)
        if [[ -n "$subcommand" ]]; then
            cmd+=("$subcommand")
        fi
        cmd+=(--baseline-rev "$baseline_sha" --package "$crate_name")
        if [[ "$supports_locked" -eq 1 ]]; then
            cmd+=("--locked")
        fi
        if [[ -n "$ignore_arg" ]]; then
            cmd+=("--ignore" "$ignore_arg")
        fi
        log "checking $crate_name ..."
        rc=0
        timeout --kill-after=30s "$crate_timeout" "${cmd[@]}" >"$log_dir/$crate_name.log" 2>&1 || rc=$?
    fi
    compared=$((compared + 1))

    case "$rc" in
        0)
            verdict="PASS"
            detail="$(grep -m1 -E 'no semver update required' "$log_dir/$crate_name.log" 2>/dev/null || true)"
            [[ -n "$detail" ]] || detail="no breaking change"
            ;;
        100)
            verdict="FAIL"
            detail="$(summarize_failures "$log_dir/$crate_name.log")"
            [[ -n "$detail" ]] || detail="semver violation"
            bump="$(grep -m1 -E 'semver requires new' "$log_dir/$crate_name.log" 2>/dev/null || true)"
            if [[ -n "$bump" ]]; then
                detail="$detail; $bump"
            fi
            ;;
        101)
            verdict="FAIL"
            detail="check could not complete: $(first_error_line "$log_dir/$crate_name.log")"
            ;;
        124)
            verdict="FAIL"
            detail="timed out after ${crate_timeout}s"
            ;;
        *)
            verdict="FAIL"
            detail="tool exited with code $rc: $(first_error_line "$log_dir/$crate_name.log")"
            ;;
    esac

    if [[ "$verdict" == "FAIL" ]]; then
        failures=$((failures + 1))
        log "  $crate_name: FAIL — $detail"
    else
        log "  $crate_name: $verdict"
    fi
    results+=("$crate_name"$'\t'"$verdict"$'\t'"$detail")
done

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------
echo ""
echo "=== semver-api-gate summary (baseline $baseline_rev) ==="
printf '%-44s %-6s %s\n' "crate" "verdict" "detail"
for row in "${results[@]}"; do
    IFS=$'\t' read -r row_name row_verdict row_detail <<<"$row"
    printf '%-44s %-6s %s\n' "$row_name" "$row_verdict" "${row_detail:0:180}"
done
echo ""

if [[ "$failures" -gt 0 ]]; then
    fail "$failures crate(s) broke the public API against $baseline_rev ($baseline_sha)."
    fail "the full per-crate logs are printed above; re-run a single crate with"
    fail "SEMVER_BASELINE_REV=$baseline_rev to reproduce it."
    exit 1
fi

if [[ "$compared" -eq 0 ]]; then
    fail "no publishable crate could be compared against $baseline_rev ($baseline_sha)."
    fail "the gate proved nothing: every publishable crate is absent from the baseline."
    fail "This is the expected state right after a package rename (the baseline still"
    fail "carries the old names) and it must not read as a green run. Point"
    fail "SEMVER_BASELINE_REV at a revision that contains the current package names,"
    fail "or publish the crates so the published release becomes the baseline."
    exit 1
fi

log "compared $compared crate(s) against $baseline_rev ($baseline_sha); every one of them kept its public API."
exit 0
