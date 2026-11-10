#!/usr/bin/env bash
# One authority for where the public-API gate puts its scratch.
#
# The gate extracts a whole baseline worktree, builds it, keeps per-crate logs
# and needs a cache. All of that is gigabytes of ordinary I/O. When the chosen
# directory cannot hold it the failure arrives as ENOSPC/EDQUOT *inside* the
# check, which the old flow printed as "broke the public API". A machine
# condition was being reported as a product verdict.
#
# R125 half-fixed it by preferring an ambient `TMPDIR`, which introduced the
# opposite failure: a quota-capped `/tmp` makes the gate *refuse to run*, and
# the harness maps exit 2 to SKIP. A blocking public-API gate quietly
# disappearing from CI is worse than a false FAIL, because nobody notices.
#
# Selection is therefore a fixed, testable order: explicit `SEMVER_SCRATCH`,
# the repository's own ignored `.tmp/semver-gate`, then `TMPDIR` (only when
# set). Each candidate must prove it can hold a real write; nothing is chosen
# without that proof. When none qualifies, exit 2 names every rejection: a loud
# refusal, never a pass and never an API verdict.
#
# Usage:
#   source scripts/quality/semver_scratch.sh
#   scratch_root="$(select_scratch_root "$repo")" || exit 2
#   bash scripts/quality/semver_scratch.sh --self-test
#
# Environment:
#   SEMVER_SCRATCH           first-choice directory
#   SEMVER_MIN_FREE_BYTES    free bytes required by `df` (default 128 MiB)
#   SEMVER_PROBE_BYTES       bytes actually written (default 128 MiB)

DEFAULT_MIN_FREE_BYTES=134217728
DEFAULT_PROBE_BYTES=134217728

# Whole bytes free on the filesystem backing a path. `df -k -P` is the POSIX
# form, so the same helper reads correctly on GNU and BSD/macOS coreutils.
available_bytes() {
    local path=$1 kb
    kb=$(df -k -P "$path" 2>/dev/null | awk 'NR==2 {print $4}')
    [[ "$kb" =~ ^[0-9]+$ ]] || return 1
    printf '%s\n' $((kb * 1024))
}

# Return 0 when a candidate can hold the gate's writes; otherwise print one
# reason on stdout. Callers capture stdout, so *every* diagnostic helper here
# must be silenced: `available_bytes` and `set -x` traces would otherwise be
# mistaken for the chosen path.
reject_reason() {
    local dir=$1 probe avail bytes need detail
    bytes=${SEMVER_PROBE_BYTES:-$DEFAULT_PROBE_BYTES}
    need=${SEMVER_MIN_FREE_BYTES:-$DEFAULT_MIN_FREE_BYTES}
    if ! mkdir -p "$dir" 2>/dev/null; then
        printf "cannot create '%s'\n" "$dir"
        return 1
    fi
    if ! avail=$(available_bytes "$dir" 2>/dev/null); then
        printf "'%s' sits on a filesystem df cannot read\n" "$dir"
        return 1
    fi
    if ((avail < need)); then
        printf "'%s' holds only %s bytes free, below %s\n" "$dir" "$avail" "$need"
        return 1
    fi
    probe=$(mktemp -d "${dir%/}/semver-probe.XXXXXX" 2>/dev/null) || {
        printf "cannot create a probe directory under '%s'\n" "$dir"
        return 1
    }
    if ! head -c "$bytes" /dev/zero >"$probe/write-probe" 2>"$probe/write-probe.err"; then
        detail=$(tail -c 200 "$probe/write-probe.err" 2>/dev/null | tr -d '\0')
        rm -rf "$probe"
        printf "'%s' failed a %s-byte write (%s); the gate would report an I/O condition as a public-API break\n" \
            "$dir" "$bytes" "${detail:-I/O error}"
        return 1
    fi
    rm -rf "$probe"
    return 0
}

# Candidates in priority order, deduplicated. An unset TMPDIR contributes
# nothing at all -- `${TMPDIR:-}/semver-gate` would become `/semver-gate`,
# a path that looks absolute and sane while meaning nothing.
scratch_candidates() {
    local repo=$1 seen="" candidate tmpdir
    tmpdir=${TMPDIR:-}
    for candidate in "${SEMVER_SCRATCH:-}" "$repo/.tmp/semver-gate" \
                     "$tmpdir" "${tmpdir:+$tmpdir/semver-gate}"; do
        [[ -n "$candidate" ]] || continue
        case "$seen" in
            *"|$candidate|") continue ;;
        esac
        seen="$seen|$candidate|"
        printf '%s\n' "$candidate"
    done
}

# Print the chosen directory; log each rejection to stderr. Return 2 when none
# qualifies, so the harness can distinguish "unavailable" from "API broke".
select_scratch_root() {
    local repo=$1 candidate reason chosen="" complaints=""
    while IFS= read -r candidate; do
        [[ -n "$candidate" ]] || continue
        if reason=$(reject_reason "$candidate"); then
            chosen=$candidate
            break
        fi
        complaints+="$reason"$'\n'
    done < <(scratch_candidates "$repo")
    if [[ -z "$chosen" ]]; then
        printf '%s' "$complaints" >&2
        echo "no scratch root is writable and roomy enough for the public-API gate" >&2
        echo "point SEMVER_SCRATCH at a filesystem with room, or free '$repo/.tmp'" >&2
        return 2
    fi
    printf '%s\n' "$chosen"
}

semver_scratch_self_test() {
    local tmp chosen rc=0 count
    # The gate runs with `set -euo pipefail`, and a self-test's whole point is
    # to collect *every* failed case. One failing assertion must not abort the
    # function before the remaining cases run, nor turn a diagnostic into a
    # bare exit 2 with no output.
    set +e
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' RETURN
    export SEMVER_PROBE_BYTES=4096 SEMVER_MIN_FREE_BYTES=1024

    # 1. Repo-owned scratch wins over an ambient TMPDIR: the ordering whose
    #    absence let a blocking gate turn into a quiet SKIP.
    chosen=$(TMPDIR="$tmp/ambient" select_scratch_root "$tmp/repo" 2>/dev/null)
    [[ "$chosen" == "$tmp/repo/.tmp/semver-gate" ]] || {
        echo "1 FAIL ambient TMPDIR outran repo scratch ($chosen)"; rc=1; }

    # 2. An explicit choice overrides, and the directory really gets created.
    chosen=$(SEMVER_SCRATCH="$tmp/explicit" select_scratch_root "$tmp/repo" 2>/dev/null)
    [[ "$chosen" == "$tmp/explicit" && -d "$tmp/explicit" ]] || {
        echo "2 FAIL explicit scratch not honoured or never created ($chosen)"; rc=1; }

    # 3. Fall-through when the explicit root cannot exist: the gate keeps
    #    running, so no complaint is printed for a candidate that was merely
    #    skipped (diagnostics are for the case where *nothing* qualifies).
    chosen=$(SEMVER_SCRATCH="/etc/hostname/nope" select_scratch_root "$tmp/repo" 2>"$tmp/log3")
    [[ "$chosen" == "$tmp/repo/.tmp/semver-gate" ]] || {
        echo "3 FAIL did not fall through ($chosen)"; rc=1; }
    [[ ! -s "$tmp/log3" ]] || { echo "3 FAIL a recovered candidate was logged as a failure"; rc=1; }

    # 4. Creatable but non-writable (0500): the real write probe must reject it.
    mkdir -p "$tmp/readonly" && chmod 500 "$tmp/readonly"
    chosen=$(SEMVER_SCRATCH="$tmp/readonly" select_scratch_root "$tmp/repo" 2>"$tmp/log4")
    [[ "$chosen" == "$tmp/repo/.tmp/semver-gate" ]] || {
        echo "4 FAIL probe-impossible candidate accepted ($chosen)"; rc=1; }
    chmod 700 "$tmp/readonly"

    # 5. No candidate can satisfy the free-space floor: loud refusal, exit 2.
    SEMVER_MIN_FREE_BYTES=1125899906842624 \
        select_scratch_root "$tmp/repo" 2>"$tmp/refusal"
    [[ $? -eq 2 ]] || { echo "5 FAIL exhausted candidates did not return 2"; rc=1; }
    grep -q "no scratch root is writable and roomy enough" "$tmp/refusal" || {
        echo "5 FAIL refusal did not explain itself"; rc=1; }
    export SEMVER_MIN_FREE_BYTES=1024

    # 6. Deduplication: a repo living inside $TMPDIR is not walked twice.
    count=$(TMPDIR="$tmp/dup" select_scratch_root "$tmp/dup" 2>/dev/null | wc -l)
    [[ "$count" -eq 1 ]] || { echo "6 FAIL candidate list not deduplicated ($count)"; rc=1; }

    # 7. Unset TMPDIR may not invent `/semver-gate`.
    unset TMPDIR
    count=$(scratch_candidates "$tmp/repo" | wc -l)
    grep -q '^/semver-gate$' <(scratch_candidates "$tmp/repo") && {
        echo "7 FAIL an unset TMPDIR invented /semver-gate"; rc=1; }
    [[ "$count" -eq 1 ]] || { echo "7 FAIL unexpected candidate count ($count)"; rc=1; }

    # 8. df reads a normal filesystem with a positive byte count.
    [[ $(available_bytes "$tmp") -gt 0 ]] || {
        echo "8 FAIL available_bytes cannot read a normal filesystem"; rc=1; }

    if ((rc == 0)); then
        echo "semver scratch selection self-test: PASS (8 cases: ordering, explicit choice, fall-through, real write probe, loud exhaustion, dedup, unset TMPDIR, df)"
    fi
    return $rc
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    if [[ "${1:-}" == "--self-test" ]]; then
        semver_scratch_self_test
        exit $?
    fi
    echo "usage: $0 --self-test" >&2
    exit 2
fi
