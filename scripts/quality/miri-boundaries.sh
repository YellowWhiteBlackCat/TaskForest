#!/usr/bin/env bash
# Miri on the three audited boundary crates (ADR-022 perf-ioctl, ADR-024
# afpacket, ADR-025 fd-bridge). These are the workspace's ONLY `unsafe`
# trust roots, so they get the Miri pass everything else skips.
#
# Miri cannot emulate real syscalls (perf_event_open, sendmsg, socket), so
# a test that stops at an "unsupported operation" is EXPECTED and is not a
# failure. What this gate rejects is "Undefined Behavior": a misaligned
# deref, dangling pointer, or invalid cast inside the audited unsafe blocks.
# fd-bridge is the standing example: this gate caught the Vec<u8>-aligned
# CMSG buffer UB that release builds shipped silently (2026-08-07).
#
# Usage: scripts/quality/miri-boundaries.sh [--full]
#   --full   run every test in the three crates (slower; hits the syscall
#            wall in each crate). Default: run, for EACH of the three
#            crates, only the pure-logic tests (no syscalls) that Miri can
#            execute to completion — a syscall-bound test stopped at the
#            wall is not coverage, so it belongs to --full, not here.

set -u

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"

MIRI=(cargo +nightly miri)

failures=0

run_crate() {
    local crate="$1"
    local filter="${2:-}"
    local out
    # shellcheck disable=SC2086
    out=$("${MIRI[@]}" test -p "$crate" $filter 2>&1)
    local rc=$?
    if grep -q "Undefined Behavior" <<<"$out"; then
        echo "FAIL Miri $crate: Undefined Behavior"
        grep -B2 -A6 "Undefined Behavior" <<<"$out" | head -20
        failures=$((failures + 1))
        return
    fi
    if grep -q "test result: ok" <<<"$out"; then
        echo "PASS Miri $crate: clean ($(grep -c '^test .* ok$' <<<"$out") tests)"
        return
    fi
    if grep -q "unsupported operation" <<<"$out"; then
        echo "PASS Miri $crate: no UB; stopped at an unemulatable syscall (expected)"
        return
    fi
    echo "FAIL Miri $crate: abnormal exit rc=$rc"
    tail -20 <<<"$out"
    failures=$((failures + 1))
}

if [[ "${1:-}" == "--full" ]]; then
    run_crate taskmanager-afpacket
    run_crate taskmanager-fd-bridge
    run_crate taskmanager-perf-ioctl
else
    # Pure-logic passes Miri can execute to completion. Every filter below
    # was verified to finish under Miri (no "unsupported operation"); the
    # syscall-bound halves of each crate stay in --full, where they stop at
    # the wall (PASS = no UB, but zero execution value).
    #
    # afpacket: the five-tuple frame parser — the whole point of the crate's
    # unsafe is feeding it bytes.
    run_crate taskmanager-afpacket "parse::"
    # fd-bridge: the audited CMSG walk (find_scm_rights) — hand-built cmsg
    # chains in aligned slabs driven through the libc CMSG_* helpers, pure
    # pointer/size arithmetic, including the fuzz-found window-overrun
    # regression (bf134488) and the 2026-08-07 misaligned-slab UB class the
    # header cites. retry_on_eintr and the ENOSYS predicate are pure wrapper
    # logic that touches no descriptor. Everything socketpair/sendmsg/
    # recvmsg/getsockopt/pidfd is a real syscall -> wall, not coverage.
    run_crate taskmanager-fd-bridge "find_scm_rights"
    run_crate taskmanager-fd-bridge "retry_on_eintr"
    run_crate taskmanager-fd-bridge "enosys"
    # perf-ioctl: the #[repr(C)] perf_event_attr layout contract (the E2BIG
    # guard) is the crate's only syscall-free face — pure size/align/offset
    # arithmetic on the ABI struct the kernel reads. open/open_enabled hit
    # the perf_event_open syscall wall.
    run_crate taskmanager-perf-ioctl "perf_event_attr_layout"
fi

if [[ $failures -gt 0 ]]; then
    echo "miri-boundaries: $failures crate(s) failed"
    exit 1
fi
echo "miri-boundaries: clean"
