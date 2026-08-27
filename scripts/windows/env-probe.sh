#!/usr/bin/env bash
# env-probe.sh — Windows (Git Bash) environment probe for the TaskForest gates.
#
# The repository's canonical automation targets the Linux workstation:
# capture-niri.sh needs a Wayland compositor, the extended tier Miri-checks
# Linux-only boundary crates, and the lease manager only redirects TMPDIR.
# This probe is the entry point of the parallel Windows suite
# (scripts/windows/). It verifies the Git Bash toolchain that suite needs,
# reports the native toolchain identity, checks that line-ending policy will
# not desync the SHA-256 source-provenance manifests, and — in `enter` mode —
# emits the eval block that redirects TMP/TEMP/TMPDIR plus CARGO_TARGET_DIR
# onto the repository drive (MSVC tooling honors TMP/TEMP, not TMPDIR).
#
# Usage:
#   bash scripts/windows/env-probe.sh                     # report + exit 0/1
#   eval "$(bash scripts/windows/env-probe.sh enter <task>)"  # lease one command
#
# Guard notes (scripts/quality/automation_safety_guard.py): pure probes, no
# background children, no inline Python; the interpreter name appears only on
# lines already prefixed by `timeout` or inside comments.
#
# Interpreter-shim recipe (if the probe misses it): Windows installs usually
# expose `python` only. Create a Git Bash shim so the repo's launcher pattern
# keeps working, then re-run this probe:
#   mkdir -p ~/.local/bin
#   printf '#!/bin/sh\nexec python "$@"\n' > ~/.local/bin/python3
#   chmod +x ~/.local/bin/python3

set -u

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"

mode="${1:-check}"
task="${2:-windows-env}"

if [[ "$mode" != "check" && "$mode" != "enter" ]]; then
    echo "usage: env-probe.sh [check|enter <task-name>]" >&2
    exit 2
fi

failures=0
notes=0

require() {
    if command -v "$1" >/dev/null 2>&1; then
        printf 'OK    %s (%s)\n' "$1" "$(command -v "$1")"
    else
        printf 'MISS  %s — %s\n' "$1" "$2"
        failures=$((failures + 1))
    fi
}

note() {
    printf 'note  %s\n' "$1"
    notes=$((notes + 1))
}

if [[ "$mode" == "enter" ]]; then
    # Reuse the shared lease manager (LEASE bookkeeping + reclaim + EXIT trap),
    # then add the TMP/TEMP exports that native Windows tooling requires.
    cat <<EOF
eval "\$("$repo/scripts/agent-workdir.sh" enter "$task")"
export TMP="\$TMPDIR"
export TEMP="\$TMPDIR"
echo "windows env lease: TMP/TEMP/TMPDIR=\$TMPDIR  CARGO_TARGET_DIR=\$CARGO_TARGET_DIR" >&2
EOF
    exit 0
fi

echo "== shell =="
kernel="$(uname -s)"
case "$kernel" in
MINGW* | MSYS* | CYGWIN*) printf 'OK    %s (Git Bash family)\n' "$kernel" ;;
*)
    printf 'MISS  %s — this suite is Windows-only; on Linux use scripts/quality/local-gates.sh\n' "$kernel"
    exit 1
    ;;
esac

echo "== hard requirements =="
require cargo "Rust toolchain (rust-toolchain.toml pins the channel)"
require rustc "compiler front end"
require git "worktree provenance"
require sha256sum "evidence hashing"
require timeout "bounded stages (GNU coreutils)"
require column "gate summary tables"
# Probed with the exact launch pattern the gates use (timeout-bounded).
if timeout 5s python3 --version >/dev/null 2>&1; then
    printf 'OK    interpreter gate (%s via timeout)\n' "$(timeout 5s python3 --version 2>/dev/null | head -1)"
else
    printf 'MISS  Python 3 interpreter — the gates launch it exactly as probed here\n'
    failures=$((failures + 1))
fi

echo "== cargo subcommands =="
if cargo nextest --version >/dev/null 2>&1; then
    printf 'OK    cargo-nextest (%s)\n' "$(cargo nextest --version 2>/dev/null | head -1)"
else
    printf 'MISS  cargo-nextest — bare `cargo test` is banned in this repository; install cargo-nextest\n'
    failures=$((failures + 1))
fi
if cargo deny --version >/dev/null 2>&1; then
    printf 'OK    cargo-deny (%s)\n' "$(cargo deny --version 2>/dev/null | head -1)"
else
    # Advisory only: the Windows standard tier skips the deny stage when the
    # tool (or its advisory database) is unavailable.
    note "cargo-deny absent — the standard tier will SKIP the deny stage"
fi

echo "== toolchain identity =="
host="$(rustc -Vv 2>/dev/null | sed -n 's/^host: //p')"
if [[ "$host" == *"windows-msvc" ]]; then
    printf 'OK    host triple %s\n' "$host"
elif [[ -n "$host" ]]; then
    note "host triple is $host, not *-windows-msvc; the MSVC toolchain is the only verified Windows configuration"
else
    note "rustc -Vv produced no host triple"
fi

echo "== line-ending policy =="
if [[ -f .gitattributes ]]; then
    printf 'OK    .gitattributes present\n'
else
    printf 'MISS  .gitattributes — CRLF checkouts desync the SHA-256 source-provenance manifests\n'
    failures=$((failures + 1))
fi
autocrlf="$(git config core.autocrlf 2>/dev/null || true)"
[[ -n "$autocrlf" ]] && note "core.autocrlf=$autocrlf (harmless while .gitattributes pins eol=lf, but *text=auto eol=lf* must stay authoritative)"
crlf_hits=0
for probe_file in Cargo.lock rust-toolchain.toml scripts/quality/local-gates.sh \
    scripts/accept-gpui-interactions.sh scripts/windows/accept-gpui-interactions.sh; do
    if [[ -f "$probe_file" ]] && grep -q $'\r' "$probe_file"; then
        printf 'MISS  %s contains CR bytes — re-checkout the file (git checkout -- <file> after fixing attributes)\n' "$probe_file"
        crlf_hits=$((crlf_hits + 1))
    fi
done
[[ "$crlf_hits" -eq 0 ]] && printf 'OK    no CR bytes in provenance-critical text files\n'
failures=$((failures + crlf_hits))

echo "== scratch space =="
df -h "$repo" 2>/dev/null | tail -1 || note "df unavailable"

echo ""
if [[ "$failures" -gt 0 ]]; then
    echo "env-probe: $failures hard requirement(s) missing, $notes note(s)"
    # The interpreter-shim recipe lives in this file's header comment so the
    # guard-visible failure line stays a pure pointer.
    echo "remedy for a missing interpreter shim: see the header comment of scripts/windows/env-probe.sh" >&2
    exit 1
fi
echo "env-probe: ready ($notes note(s))"
