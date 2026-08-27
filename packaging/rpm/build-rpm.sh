#!/usr/bin/env bash
# Build the TaskForest .rpm from the staged release tree.
#
# The staged /usr tree comes from packaging/linux/stage-release-tree.sh (the
# PKGBUILD layout authority). It is packed verbatim as Source0 and unpacked
# straight into the build root; the spec contributes metadata only. This runs
# on the CI ubuntu host, so the brp/dependency machinery is disabled in the
# spec and the Requires list is explicit.
#
# Usage: packaging/rpm/build-rpm.sh STAGED_TREE VERSION OUTPUT_RPM
set -euo pipefail
export LC_ALL=C

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$script_dir/../.." && pwd)
mkdir -p "$repo/.tmp"

if [[ $# -ne 3 ]]; then
    echo "usage: $0 STAGED_TREE VERSION OUTPUT_RPM" >&2
    exit 2
fi
staged=$1
version=$2
output=$3

# RPM's Version field forbids dashes; a Cargo prerelease like 0.1.0-rc.1
# becomes 0.1.0~rc.1 so the final 0.1.0 release sorts above it (rpmvercmp).
# The replacement is quoted: an unquoted bare '~' inside ${//} expands to $HOME.
rpm_version=${version//'-'/'~'}

[[ -d "$staged/usr" ]] || { echo "build-rpm: $staged does not contain a staged usr/ tree" >&2; exit 1; }
command -v rpmbuild >/dev/null || { echo "build-rpm: rpmbuild not installed (apt-get install rpm)" >&2; exit 1; }

work=$(mktemp -d "$repo/.tmp/build-rpm.XXXXXX")
trap 'rm -rf "$work"' EXIT
topdir="$work/rpmbuild"
mkdir -p "$topdir"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}

# tar with top-level usr/ so the spec's %install can extract straight into
# the build root.
tar -C "$staged" -czf "$topdir/SOURCES/taskforest-tree.tar.gz" usr
cp "$script_dir/taskforest.spec" "$topdir/SPECS/"

rpmbuild -bb \
    --define "_topdir $topdir" \
    --define "version $rpm_version" \
    --define "packager TaskForest contributors" \
    "$topdir/SPECS/taskforest.spec" >/dev/null

built=$(find "$topdir/RPMS" -name '*.rpm' -type f | head -n1)
[[ -n "$built" ]] || { echo "build-rpm: rpmbuild produced no package" >&2; exit 1; }
mv "$built" "$output"
echo "build-rpm: $(basename "$output") ready ($(du -h "$output" | cut -f1))"
