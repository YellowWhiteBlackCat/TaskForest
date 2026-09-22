#!/usr/bin/env bash
# Build the TaskForest .rpm from the staged release tree.
#
# The staged /usr tree comes from packaging/linux/stage-release-tree.sh (the
# PKGBUILD layout authority). It is packed verbatim as Source0 and unpacked
# straight into the build root; the spec contributes metadata only. This runs
# on the CI ubuntu host, so the brp/dependency machinery is disabled in the
# spec and the Requires list is explicit.
#
# Usage: packaging/rpm/build-rpm.sh STAGED_TREE VERSION OUTPUT_RPM [UI]
set -euo pipefail
export LC_ALL=C

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$script_dir/../.." && pwd)
mkdir -p "$repo/.tmp"

if [[ $# -ne 3 && $# -ne 4 ]]; then
    echo "usage: $0 STAGED_TREE VERSION OUTPUT_RPM [UI]" >&2
    exit 2
fi
staged=$1
version=$2
output=$3
ui=${4:-G}

case "$ui" in
    G|g)
        spec_file="$script_dir/taskforest.spec"
        ui_tag="G"
        ;;
    I|i)
        spec_file="$script_dir/taskforest-i.spec"
        ui_tag="I"
        ;;
    T|t)
        spec_file="$script_dir/taskforest-t.spec"
        ui_tag="T"
        ;;
    B|b)
        spec_file="$script_dir/taskforest-b.spec"
        ui_tag="B"
        ;;
    C|c|common)
        spec_file="$script_dir/taskforest-common.spec"
        ui_tag="C"
        ;;
    *) echo "build-rpm: unknown UI target '$ui' (expected G, I, T, B, or C)" >&2; exit 1 ;;
esac

# RPM's Version field forbids dashes; a Cargo prerelease like 0.1.0-rc5
# becomes 0.1.0~rc5 so the final 0.1.0 release sorts above it (rpmvercmp).
# The replacement is quoted: an unquoted bare '~' inside ${//} expands to $HOME.
rpm_version=${version//'-'/'~'}

[[ -d "$staged/usr" ]] || { echo "build-rpm: $staged does not contain a staged usr/ tree" >&2; exit 1; }
command -v rpmbuild >/dev/null || { echo "build-rpm: rpmbuild not installed (apt-get install rpm)" >&2; exit 1; }

work=$(mktemp -d "$repo/.tmp/build-rpm.XXXXXX")
trap 'rm -rf "$work"' EXIT
topdir="$work/rpmbuild"
mkdir -p "$topdir"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}

# Prepare the UI-specific staged tree.
# For GPUI (G), the PKGBUILD release tree ($staged) contains the full layout authority.
# For Iced, TUI, and Bevy, if the provided staged tree does not have the frontend binary,
# generate its dedicated standalone staging tree using the respective build-deb-*.sh --stage-only.
target_staged="$staged"
case "$ui_tag" in
    G)
        [[ -f "$target_staged/usr/bin/taskforest-g" ]] || {
            echo "build-rpm: $target_staged is missing /usr/bin/taskforest-g" >&2
            exit 1
        }
        ;;
    I)
        if [[ ! -f "$target_staged/usr/bin/taskforest-i" ]]; then
            mkdir -p "$work/stage-i"
            "$repo/packaging/debian/build-deb-iced.sh" --stage-only "$work/stage-i" >/dev/null
            target_staged="$work/stage-i"
        fi
        ;;
    T)
        if [[ ! -f "$target_staged/usr/bin/taskforest-t" ]]; then
            mkdir -p "$work/stage-t"
            "$repo/packaging/debian/build-deb-tui.sh" --stage-only "$work/stage-t" >/dev/null
            target_staged="$work/stage-t"
        fi
        ;;
    B)
        if [[ ! -f "$target_staged/usr/bin/taskforest-b" ]]; then
            mkdir -p "$work/stage-b"
            "$repo/packaging/debian/build-deb-bevy.sh" --stage-only "$work/stage-b" >/dev/null
            target_staged="$work/stage-b"
        fi
        ;;
    C)
        # The common package owns only the shared data assets; it never reuses
        # a frontend staged tree. Regenerate its small dedicated tree so the
        # spec's %files matches Source0 exactly.
        mkdir -p "$work/stage-common"
        "$repo/packaging/debian/build-deb-common.sh" --stage-only "$work/stage-common" >/dev/null
        target_staged="$work/stage-common"
        ;;
esac

# For a frontend product, drop the destinations owned by taskforest-common so
# the spec's %files never lists a path another package owns; the manifest is
# the authority for that set. Empty parent directories are pruned so no
# orphaned icon directory reaches the RPM either. The common package itself
# keeps its full tree. tar uses top-level usr/ so the spec's %install can
# extract straight into the build root.
if [[ "$ui_tag" != "C" ]]; then
    manifest="$repo/docs/system-install-manifest.tsv"
    [[ -f "$manifest" ]] || { echo "build-rpm: missing $manifest" >&2; exit 1; }
    filtered="$work/staged-filtered"
    cp -a "$target_staged" "$filtered"
    while IFS= read -r destination; do
        [[ "$destination" == /usr/* ]] || continue
        rm -f "$filtered$destination"
        rmdir -p --ignore-fail-on-non-empty "$(dirname "$filtered$destination")" \
            2>/dev/null || true
    done < <(awk -F'\t' -v provider="packaging/rpm/taskforest-common.spec" \
        'NR > 1 && index($7, provider) { print $4 }' "$manifest")
    target_staged="$filtered"
fi
tar -C "$target_staged" -czf "$topdir/SOURCES/taskforest-tree.tar.gz" usr
spec_name=$(basename "$spec_file")
cp "$spec_file" "$topdir/SPECS/$spec_name"

rpmbuild -bb \
    --define "_topdir $topdir" \
    --define "version $rpm_version" \
    --define "packager TaskForest contributors" \
    "$topdir/SPECS/$spec_name" >/dev/null

built=$(find "$topdir/RPMS" -name '*.rpm' -type f | head -n1)
[[ -n "$built" ]] || { echo "build-rpm: rpmbuild produced no package" >&2; exit 1; }
mv "$built" "$output"
echo "build-rpm: $(basename "$output") ready ($(du -h "$output" | cut -f1))"
