#!/usr/bin/env bash
# Stage the release /usr tree consumed by the deb and rpm package builders.
#
# packaging/arch/PKGBUILD::package() is the single layout authority for every
# Linux package format. This script replays its install lines VERBATIM against
# the locally built release binaries — the same extraction stage-package-sim.sh
# validates — so the Arch package, the .deb, and the .rpm install byte-for-byte
# identical trees by construction instead of by review.
#
# What it does:
#   1. extracts every `install -Dm` line from PKGBUILD package() and runs it
#      verbatim (with $pkgdir/$pkgname bound) into the requested output dir;
#   2. verifies every staged polkit exec.path annotation resolves to a staged
#      helper and that nothing landed outside /usr.
#
# Prerequisites: release binaries must exist under target/release —
# taskforest-g, taskmanager-setup-helper,
# taskmanager-privilege-helper, taskmanager-net-launcher, and
# taskmanager-process-control-helper (the PKGBUILD build() set) — plus
# python3, which regenerates the third-party notices file.
#
# Usage: packaging/linux/stage-release-tree.sh OUTPUT_DIR
set -euo pipefail
export LC_ALL=C

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo"
mkdir -p "$repo/.tmp"

if [[ $# -ne 1 ]]; then
    echo "usage: $0 OUTPUT_DIR" >&2
    exit 2
fi
pkgdir=$1
mkdir -p "$pkgdir"
pkgdir=$(cd "$pkgdir" && pwd)

# The third-party notices file is a build artifact of the dependency closure:
# regenerate it beside the release binaries (the same step PKGBUILD build()
# runs) so the replayed install lines below never depend on a stale copy.
python3 scripts/gen_third_party_notices.py target/release/THIRD-PARTY-NOTICES.txt

# makepkg's package() environment: the lines below reference exactly these.
pkgname=taskforest-git
export pkgname pkgdir

package_body=$(mktemp .tmp/stage-release-tree.body.XXXXXX)
trap 'rm -f "$package_body"' EXIT
sed -n '/^package() {/,/^}/p' packaging/arch/PKGBUILD \
    | sed -e ':join' -e '/\\$/{N; s/\\[[:space:]]*\n[[:space:]]*/ /; b join}' \
          -e 's/^[[:space:]]*//' >"$package_body"
mapfile -t install_lines < <(grep -E '^install -Dm' "$package_body")
if (( ${#install_lines[@]} == 0 )); then
    echo "stage-release-tree: FAIL — no install lines found in package()" >&2
    exit 1
fi
for line in "${install_lines[@]}"; do
    eval "$line"
done
# Nothing may land outside the /usr prefix the package formats own.
outside=$(cd "$pkgdir" && find . -mindepth 1 -maxdepth 1 ! -name usr)
if [[ -n "$outside" ]]; then
    echo "stage-release-tree: FAIL — staged paths outside /usr:" >&2
    echo "$outside" >&2
    exit 1
fi

# Every staged policy's exec.path annotation must resolve to a staged helper.
resolved=0
while IFS= read -r policy; do
    while IFS= read -r annotated; do
        if [[ ! -f "$pkgdir$annotated" ]]; then
            echo "stage-release-tree: FAIL — $policy annotates $annotated, not staged" >&2
            exit 1
        fi
        resolved=$((resolved + 1))
    done < <(sed -n 's/.*org\.freedesktop\.policykit\.exec\.path">\([^<]*\)<.*/\1/p' "$policy")
done < <(find "$pkgdir/usr/share/polkit-1/actions" -type f -name '*.policy' 2>/dev/null || true)
if (( resolved == 0 )); then
    echo "stage-release-tree: FAIL — no polkit exec.path annotations found" >&2
    exit 1
fi

staged_count=$(cd "$pkgdir" && find . \( -type f -o -type l \) | wc -l)
echo "stage-release-tree: ${staged_count} destinations staged under $pkgdir/usr," \
     "${resolved} polkit exec.path annotations resolve to staged helpers"
