#!/usr/bin/env bash
# Build the taskforest-daemon .deb package.
#
# Usage:
#   packaging/debian/build-deb-daemon.sh [VERSION] [OUTPUT_DEB] [DEB_ARCH]
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
        version="0.2.3"
    fi
fi
deb_version=${version//'-'/'~'}

if [[ -z "$deb_arch" ]]; then
    case "$(uname -m)" in
        x86_64|amd64) deb_arch="amd64" ;;
        aarch64|arm64) deb_arch="arm64" ;;
        *) deb_arch="amd64" ;;
    esac
fi

arch_label="x64"
[[ "$deb_arch" == "arm64" ]] && arch_label="arm64"
if [[ -z "$output" ]]; then
    output="$repo/TaskForest-Daemon-${version}-${arch_label}.deb"
fi

work=$(mktemp -d "$repo/.tmp/build-deb-daemon.XXXXXX")
trap 'rm -rf "$work"' EXIT

# Directories
mkdir -p "$work/usr/lib/systemd/system"
mkdir -p "$work/usr/lib/systemd/user"
mkdir -p "$work/DEBIAN"

# Install service files
cp "$repo/packaging/daemon/taskforest-daemon.service" "$work/usr/lib/systemd/system/taskforest-daemon.service"
cp "$repo/packaging/daemon/taskforest-daemon.user.service" "$work/usr/lib/systemd/user/taskforest-daemon.service"

installed_size=$(du -sk "$work" | cut -f1)

sed -e "s/__VERSION__/$deb_version/g" \
    -e "s/__ARCH__/$deb_arch/g" \
    -e "s/__INSTALLED_SIZE__/$installed_size/g" \
    "$repo/packaging/debian/control-daemon" > "$work/DEBIAN/control"

if [[ -n "$stage_only" ]]; then
    mkdir -p "$stage_only"
    cp -a "$work/." "$stage_only/"
    exit 0
fi

dpkg-deb --build --root-owner-group "$work" "$output" >/dev/null 2>&1 || {
    # Fallback to ar/tar if dpkg-deb not available
    tar -czf "$repo/.tmp/control.tar.gz" -C "$work/DEBIAN" .
    tar -czf "$repo/.tmp/data.tar.gz" --exclude='./DEBIAN' -C "$work" .
    echo "2.0" > "$repo/.tmp/debian-binary"
    ar rcs "$output" "$repo/.tmp/debian-binary" "$repo/.tmp/control.tar.gz" "$repo/.tmp/data.tar.gz"
}
