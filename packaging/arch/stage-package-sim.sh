#!/usr/bin/env bash
# No-sudo staging simulation of packaging/arch/PKGBUILD::package() — the
# Packaging receipt procedure, governed by docs/SYSTEM_INSTALL_MANIFEST.md,
# is mechanized here so it can be re-run after every package() change.
#
# What it does:
#   1. extracts every `install -Dm` line from package() and runs it VERBATIM
#      (with $pkgdir/$pkgname bound) against the locally built release
#      artifacts + repo assets, into a throwaway staging dir;
#   2. verifies every staged destination is allowlisted in
#      docs/system-install-manifest.tsv (empty diff vs the guard's whitelist);
#   3. verifies each staged file carries the mode its install line set
#      (0755 binaries / 0644 assets);
#   4. verifies every staged polkit policy's org.freedesktop.policykit.exec.path
#      annotation resolves to an actually-staged helper.
#
# Prerequisites: the release binaries from build() must already exist under
# target/release (the GPUI shape artifact plus the setup, privilege, network,
# and process-control helpers; the package surface is GPUI-only).
#
# Usage: packaging/arch/stage-package-sim.sh [staging-dir]
# (default: a self-cleaning directory under the repo's gitignored .tmp/)
set -euo pipefail
export LC_ALL=C

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo"

if [[ $# -gt 1 ]]; then
    echo "usage: $0 [staging-dir]" >&2
    exit 2
fi
pkgdir=${1:-}
cleanup=0
if [[ -z "$pkgdir" ]]; then
    mkdir -p .tmp
    pkgdir=$(mktemp -d .tmp/stage-package-sim.XXXXXX)
    cleanup=1
fi
mkdir -p "$pkgdir"
pkgdir=$(cd "$pkgdir" && pwd)
trap 'if (( cleanup )); then rm -rf "$pkgdir"; fi' EXIT

# makepkg's package() environment: the lines below reference exactly these.
pkgname=taskforest-git
pkgdir="$pkgdir"
export pkgname pkgdir

# --- 1. run every install line of package() verbatim ------------------------
# The PKGBUILD is reviewed repo content (makepkg sources it wholesale), so
# evaluating its own install lines is the same trust boundary as the package
# build itself. Backslash continuations are joined into one logical line first
# so a two-line `install` is parsed as a single command.
package_body=$(mktemp .tmp/stage-package-sim.body.XXXXXX)
trap 'rm -f "$package_body"; if (( cleanup )); then rm -rf "$pkgdir"; fi' EXIT
sed -n '/^package() {/,/^}/p' packaging/arch/PKGBUILD \
    | sed -e ':join' -e '/\\$/{N; s/\\[[:space:]]*\n[[:space:]]*/ /; b join}' \
          -e 's/^[[:space:]]*//' >"$package_body"
mapfile -t install_lines < <(grep -E '^install -Dm' "$package_body")
rm -f "$package_body"
if (( ${#install_lines[@]} == 0 )); then
    echo "stage-package-sim: FAIL — no install lines found in package()" >&2
    exit 1
fi
for line in "${install_lines[@]}"; do
    eval "$line"
done
# --- 2. every staged destination must be manifest-allowlisted ---------------
manifest_paths=$(mktemp .tmp/stage-package-sim.manifest.XXXXXX)
trap 'rm -f "$manifest_paths"; if (( cleanup )); then rm -rf "$pkgdir"; fi' EXIT
cut -f4 docs/system-install-manifest.tsv | tail -n +2 | sort >"$manifest_paths"

staged_paths=$(mktemp .tmp/stage-package-sim.staged.XXXXXX)
trap 'rm -f "$manifest_paths" "$staged_paths"; if (( cleanup )); then rm -rf "$pkgdir"; fi' EXIT
(cd "$pkgdir" && find . \( -type f -o -type l \) | sed 's#^\.##' | sort) >"$staged_paths"

unlisted=$(comm -23 "$staged_paths" "$manifest_paths")
if [[ -n "$unlisted" ]]; then
    echo "stage-package-sim: FAIL — staged paths missing from the manifest whitelist:" >&2
    echo "$unlisted" >&2
    exit 1
fi
staged_count=$(wc -l <"$staged_paths")

# --- 3. modes must match what each install line set -------------------------
binaries=0
assets=0
for line in "${install_lines[@]}"; do
    mode=$(sed -n 's/.*install -Dm\([0-9]*\).*/\1/p' <<<"$line")
    stripped=${line%\"}
    dest=${stripped##*\"}
    dest=${dest/\$pkgdir/$pkgdir}
    dest=${dest/\$pkgname/$pkgname}
    actual=$(stat -c '%a' "$dest")
    if [[ "$actual" != "$mode" ]]; then
        echo "stage-package-sim: FAIL — $dest is $actual, install line set $mode" >&2
        exit 1
    fi
    if [[ "$mode" == "755" ]]; then
        binaries=$((binaries + 1))
    else
        assets=$((assets + 1))
    fi
done

# --- 4. staged policies' exec.path annotations resolve to staged helpers ----
resolved=0
while IFS= read -r policy; do
    while IFS= read -r annotated; do
        if [[ ! -f "$pkgdir$annotated" ]]; then
            echo "stage-package-sim: FAIL — $policy annotates $annotated, not staged" >&2
            exit 1
        fi
        resolved=$((resolved + 1))
    done < <(sed -n 's/.*org\.freedesktop\.policykit\.exec\.path">\([^<]*\)<.*/\1/p' "$policy")
done < <(find "$pkgdir/usr/share/polkit-1/actions" -type f -name '*.policy' 2>/dev/null || true)
if (( resolved == 0 )); then
    echo "stage-package-sim: FAIL — no polkit exec.path annotations found in staged policies" >&2
    exit 1
fi

echo "stage-package-sim: ${staged_count}/${#install_lines[@]} destinations staged," \
     "${binaries} binaries at 0755 + ${assets} assets at 0644, manifest diff empty," \
     "${resolved} polkit exec.path annotations resolve to staged helpers"
