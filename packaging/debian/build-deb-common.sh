#!/usr/bin/env bash
# Build the taskforest-common .deb package.
#
# taskforest-common is the data-only package that owns the assets shared by the
# four frontend products. It exists so exactly one DEB owns each shared path:
# the frontend packages depend on it and no longer install their own
# byte-identical copies (docs/SYSTEM_INSTALL_MANIFEST.md).
#
# It carries no executable and no privileged asset, so it has no binary
# build prerequisite. The Arch package (packaging/arch/PKGBUILD) remains
# monolithic and ships its own icons; the DEB/RPM common package is the
# single owner for the split package formats.
#
# Usage:
#   packaging/debian/build-deb-common.sh [VERSION] [OUTPUT_DEB] [DEB_ARCH]
#   packaging/debian/build-deb-common.sh
#   packaging/debian/build-deb-common.sh --stage-only STAGE_DIR
set -euo pipefail
export LC_ALL=C

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$script_dir/../.." && pwd)
mkdir -p "$repo/.tmp"

stage_only=""
if [[ $# -ge 2 && "$1" == "--stage-only" ]]; then
    stage_only="$2"
    shift 2
fi

version=""
output=""
deb_arch=""

if [[ $# -eq 0 ]]; then
    : # all inferred below
elif [[ $# -eq 1 ]]; then
    version="$1"
elif [[ $# -eq 2 ]]; then
    version="$1"
    output="$2"
elif [[ $# -eq 3 ]]; then
    version="$1"
    output="$2"
    deb_arch="$3"
else
    echo "usage: $0 [VERSION] [OUTPUT_DEB] [DEB_ARCH]" >&2
    exit 2
fi

if [[ -z "$version" ]]; then
    version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$repo/Cargo.toml" | head -n1)
    if [[ -z "$version" ]]; then
        echo "build-deb-common: unable to infer version from Cargo.toml" >&2
        exit 1
    fi
fi

# Debian sorts a plain revision above the no-revision upstream version, so a
# Cargo prerelease like 0.1.0-rc5 becomes 0.1.0~rc5.
deb_version=${version//'-'/'~'}

if [[ -z "$deb_arch" ]]; then
    case "$(uname -m)" in
        x86_64|amd64) deb_arch="amd64" ;;
        aarch64|arm64) deb_arch="arm64" ;;
        *) deb_arch="amd64" ;;
    esac
fi

case "$deb_arch" in
    amd64|x64|x86_64) deb_arch="amd64" ;;
    arm64|aarch64) deb_arch="arm64" ;;
    *) echo "build-deb-common: unsupported Debian architecture '$deb_arch'" >&2; exit 1 ;;
esac

arch_label="x64"
[[ "$deb_arch" == "arm64" ]] && arch_label="arm64"

if [[ -z "$output" ]]; then
    output="$repo/TaskForest-Common-${version}-${arch_label}.deb"
fi

work=$(mktemp -d "$repo/.tmp/build-deb-common.XXXXXX")
trap 'rm -rf "$work"' EXIT

icon_src="$repo/packaging/linux/io.github.YellowWhiteBlackCat.TaskForest.svg"
[[ -s "$icon_src" ]] || { echo "build-deb-common: icon file missing or empty: $icon_src" >&2; exit 1; }

mkdir -p "$work/usr/share/icons/hicolor/scalable/apps"

install -Dm644 "$icon_src" "$work/usr/share/icons/hicolor/scalable/apps/taskforest-taskboard.svg"
for size in 16 24 32 48 64 128 256 512; do
    icon_png="$repo/packaging/linux/icons/hicolor/${size}x${size}/apps/taskforest-taskboard.png"
    [[ -s "$icon_png" ]] || { echo "build-deb-common: missing icon ladder asset: $icon_png" >&2; exit 1; }
    install -Dm644 "$icon_png" "$work/usr/share/icons/hicolor/${size}x${size}/apps/taskforest-taskboard.png"
done

# Normalize directory and file permissions.
chmod 755 "$work" "$work/usr"
find "$work/usr" -type d -exec chmod 755 {} +
find "$work/usr" -type f -exec chmod a+r {} +

if [[ -n "$stage_only" ]]; then
    mkdir -p "$stage_only"
    cp -a "$work/usr" "$stage_only/"
    echo "build-deb-common: staged tree written to $stage_only/usr"
    exit 0
fi

# Installed-Size is measured in KiB over the data payload.
installed_size=$(du -sk --exclude=DEBIAN "$work/usr" | cut -f1)
mkdir -p "$work/DEBIAN"
chmod 755 "$work/DEBIAN"

# control-common carries fixed `Replaces`/`Breaks` on the pre-split frontend
# packages (taskforest/-i/-b, all earlier than 0.2.0). They let dpkg move the
# shared hicolor icon out of an installed frontend into this data package
# instead of failing the upgrade with "trying to overwrite ... which is also in
# package ..." (Debian Policy 7.6.1). The 0.2.0 boundary is the first release
# that ships taskforest-common (CHANGELOG 0.2.0); it names the split and must
# not be replaced with __VERSION__, which would float and eventually break a
# compatible frontend.
control_src="$script_dir/control-common"
[[ -f "$control_src" ]] || { echo "build-deb-common: missing control template $control_src" >&2; exit 1; }

sed -e "s/__VERSION__/$deb_version/" \
    -e "s/__ARCH__/$deb_arch/" \
    -e "s/__INSTALLED_SIZE__/$installed_size/" \
    "$control_src" >"$work/DEBIAN/control"
chmod 644 "$work/DEBIAN/control"

mkdir -p "$(dirname "$output")"

if command -v dpkg-deb >/dev/null 2>&1; then
    dpkg-deb --root-owner-group --build "$work" "$output" >/dev/null
elif command -v ar >/dev/null 2>&1 && command -v tar >/dev/null 2>&1; then
    echo "2.0" > "$work/debian-binary"
    tar --numeric-owner --owner=0 --group=0 -czf "$work/control.tar.gz" -C "$work/DEBIAN" .
    tar --numeric-owner --owner=0 --group=0 -czf "$work/data.tar.gz" -C "$work" usr
    rm "$output" 2>/dev/null || true
    ar -qc "$output" "$work/debian-binary" "$work/control.tar.gz" "$work/data.tar.gz"
else
    echo "build-deb-common: neither dpkg-deb nor ar/tar is available to create .deb package" >&2
    exit 1
fi

echo "build-deb-common: $(basename "$output") ready ($(du -h "$output" | cut -f1))"
