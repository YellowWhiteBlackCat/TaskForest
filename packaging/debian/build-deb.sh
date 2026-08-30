#!/usr/bin/env bash
# Build the TaskForest .deb from the staged release tree.
#
# The staged /usr tree comes from packaging/linux/stage-release-tree.sh, which
# replays the PKGBUILD layout authority verbatim — this wrapper only adds the
# DEBIAN/control metadata and calls dpkg-deb. No maintainer scripts: modern dpkg
# file triggers already reload systemd user units and polkit actions, and the
# package intentionally runs no install-time privileged code.
#
# Usage: packaging/debian/build-deb.sh STAGED_TREE VERSION OUTPUT_DEB [DEB_ARCH]
# (STAGED_TREE is the directory containing usr/; it is copied, never mutated.)
set -euo pipefail
export LC_ALL=C

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$script_dir/../.." && pwd)
mkdir -p "$repo/.tmp"

if [[ $# -lt 3 || $# -gt 4 ]]; then
    echo "usage: $0 STAGED_TREE VERSION OUTPUT_DEB [DEB_ARCH]" >&2
    exit 2
fi
staged=$1
version=$2
output=$3
deb_arch=${4:-amd64}

# Debian sorts a plain revision above the no-revision upstream version, so a
# Cargo prerelease like 0.1.0-rc5 becomes 0.1.0~rc5 — below the final
# release, matching the rpm conversion in build-rpm.sh. The replacement is
# quoted: an unquoted bare '~' inside ${//} expands to $HOME.
deb_version=${version//'-'/'~'}

case "$deb_arch" in
    amd64|arm64) ;;
    *) echo "build-deb: unsupported Debian architecture '$deb_arch'" >&2; exit 1 ;;
esac

[[ -d "$staged/usr" ]] || { echo "build-deb: $staged does not contain a staged usr/ tree" >&2; exit 1; }
command -v dpkg-deb >/dev/null || { echo "build-deb: dpkg-deb not installed" >&2; exit 1; }

work=$(mktemp -d "$repo/.tmp/build-deb.XXXXXX")
trap 'rm -rf "$work"' EXIT
cp -a "$staged/usr" "$work/usr"
# A staged tree copied from an NTFS-mounted checkout can carry mode 0777.
# Normalize the package-owned directory metadata before dpkg-deb validates the
# DEBIAN control directory; native Linux runners already have these modes.
chmod 755 "$work" "$work/usr"

# Installed-Size is measured in KiB over the data payload.
installed_size=$(du -sk --exclude=DEBIAN "$work/usr" | cut -f1)
mkdir -p "$work/DEBIAN"
chmod 755 "$work/DEBIAN"
sed -e "s/__VERSION__/$deb_version/" \
    -e "s/__ARCH__/$deb_arch/" \
    -e "s/__INSTALLED_SIZE__/$installed_size/" \
    "$script_dir/control" >"$work/DEBIAN/control"

# --root-owner-group keeps a non-root build honest: every archive entry is
# recorded as root:root exactly as a real package transaction would install it.
dpkg-deb --root-owner-group --build "$work" "$output" >/dev/null
echo "build-deb: $(basename "$output") ready ($(du -h "$output" | cut -f1))"
