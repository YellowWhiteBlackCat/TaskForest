#!/usr/bin/env bash
# Build the TaskForestI (Iced frontend) .deb package.
#
# Unlike the GPUI package which replayed the multi-helper PKGBUILD release tree,
# this script packages the standalone Iced frontend product (`taskforest-i`)
# along with its desktop entry, AppStream metainfo, icon, and license notices.
# Polkit helpers are shared across frontends under /usr/libexec/taskforest-* and
# can be installed via the primary taskforest package or standalone scripts.
#
# Strict Wayland-only: A Wayland session is required and X11 is not supported.
# Zero X11 packages, zero X11 dependencies.
#
# Usage:
#   packaging/debian/build-deb-iced.sh [BINARY_OR_STAGED_TREE] [VERSION] [OUTPUT_DEB] [DEB_ARCH]
#   packaging/debian/build-deb-iced.sh VERSION OUTPUT_DEB [DEB_ARCH]
#   packaging/debian/build-deb-iced.sh
#   packaging/debian/build-deb-iced.sh --stage-only STAGE_DIR [BINARY]
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

target_src=""
version=""
output=""
deb_arch=""

if [[ $# -eq 0 ]]; then
    : # all inferred below
elif [[ $# -eq 1 ]]; then
    if [[ -e "$1" ]]; then
        target_src="$1"
    else
        version="$1"
    fi
elif [[ $# -eq 2 ]]; then
    if [[ -e "$1" ]]; then
        target_src="$1"
        version="$2"
    else
        version="$1"
        output="$2"
    fi
elif [[ $# -eq 3 ]]; then
    if [[ "$2" =~ \.deb$ && ( "$3" =~ ^(amd64|arm64|x64|x86_64|aarch64)$ ) ]]; then
        version="$1"
        output="$2"
        deb_arch="$3"
    elif [[ -e "$1" ]]; then
        target_src="$1"
        version="$2"
        output="$3"
    else
        version="$1"
        output="$2"
        deb_arch="$3"
    fi
elif [[ $# -eq 4 ]]; then
    target_src="$1"
    version="$2"
    output="$3"
    deb_arch="$4"
else
    echo "usage: $0 [BINARY_OR_STAGED_TREE] [VERSION] [OUTPUT_DEB] [DEB_ARCH]" >&2
    exit 2
fi

if [[ -z "$version" ]]; then
    version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$repo/Cargo.toml" | head -n1)
    if [[ -z "$version" ]]; then
        echo "build-deb-iced: unable to infer version from Cargo.toml" >&2
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
    *) echo "build-deb-iced: unsupported Debian architecture '$deb_arch'" >&2; exit 1 ;;
esac

arch_label="x64"
[[ "$deb_arch" == "arm64" ]] && arch_label="arm64"

if [[ -z "$output" ]]; then
    output="$repo/TaskForest-I-${version}-${arch_label}.deb"
fi

work=$(mktemp -d "$repo/.tmp/build-deb-iced.XXXXXX")
trap 'rm -rf "$work"' EXIT

if [[ -n "$target_src" && -d "$target_src/usr" ]]; then
    cp -a "$target_src/usr" "$work/usr"
else
    bin=""
    if [[ -n "$target_src" && -f "$target_src" ]]; then
        bin="$target_src"
    elif [[ -n "${TASKFOREST_I_BIN:-}" && -f "$TASKFOREST_I_BIN" ]]; then
        bin="$TASKFOREST_I_BIN"
    elif [[ -f "$repo/target/release/taskforest-i" ]]; then
        bin="$repo/target/release/taskforest-i"
    elif [[ -f "$repo/target/debug/taskforest-i" ]]; then
        bin="$repo/target/debug/taskforest-i"
    fi

    if [[ -z "$bin" || ! -f "$bin" ]]; then
        echo "build-deb-iced: release binary not found; building taskmanager-iced..."
        cargo build --locked --release -p taskmanager-iced -j "${CARGO_BUILD_JOBS:-4}" 2>/dev/null || \
        cargo build --release -p taskmanager-iced -j "${CARGO_BUILD_JOBS:-4}"
        bin="$repo/target/release/taskforest-i"
    fi

    if [[ ! -f "$bin" ]]; then
        echo "build-deb-iced: binary not found: $bin" >&2
        exit 1
    fi

    [[ -x "$bin" ]] || { echo "build-deb-iced: $bin is not executable" >&2; exit 1; }

    desktop_src="$repo/packaging/linux/io.github.YellowWhiteBlackCat.TaskForestI.desktop"
    metainfo_src="$repo/packaging/linux/io.github.YellowWhiteBlackCat.TaskForestI.metainfo.xml"
    icon_src="$repo/packaging/linux/io.github.YellowWhiteBlackCat.TaskForest.svg"

    [[ -f "$desktop_src" ]] || { echo "build-deb-iced: desktop file missing: $desktop_src" >&2; exit 1; }
    [[ -f "$metainfo_src" ]] || { echo "build-deb-iced: metainfo file missing: $metainfo_src" >&2; exit 1; }
    [[ -s "$icon_src" ]] || { echo "build-deb-iced: icon file missing or empty: $icon_src" >&2; exit 1; }

    mkdir -p "$work/usr/bin" \
        "$work/usr/share/applications" \
        "$work/usr/share/metainfo" \
        "$work/usr/share/icons/hicolor/scalable/apps" \
        "$work/usr/share/licenses/taskforest-i"

    install -m755 "$bin" "$work/usr/bin/taskforest-i"
    install -m644 "$desktop_src" "$work/usr/share/applications/io.github.YellowWhiteBlackCat.TaskForestI.desktop"
    install -m644 "$metainfo_src" "$work/usr/share/metainfo/io.github.YellowWhiteBlackCat.TaskForestI.metainfo.xml"
    install -m644 "$icon_src" "$work/usr/share/icons/hicolor/scalable/apps/taskforest-taskboard.svg"

    if command -v strip >/dev/null 2>&1; then
        strip --strip-unneeded "$work/usr/bin/taskforest-i" 2>/dev/null || \
        strip -s "$work/usr/bin/taskforest-i" 2>/dev/null || true
    fi

    # Ensure third-party notices exist
    if [[ ! -f "$repo/target/release/THIRD-PARTY-NOTICES.txt" ]] && [[ -f "$repo/scripts/gen_third_party_notices.py" ]]; then
        python3 "$repo/scripts/gen_third_party_notices.py" "$repo/target/release/THIRD-PARTY-NOTICES.txt" 2>/dev/null || true
    fi

    if [[ -f "$repo/LICENSE" ]]; then
        install -m644 "$repo/LICENSE" "$work/usr/share/licenses/taskforest-i/LICENSE"
    fi
    if [[ -f "$repo/target/release/THIRD-PARTY-NOTICES.txt" ]]; then
        install -m644 "$repo/target/release/THIRD-PARTY-NOTICES.txt" \
            "$work/usr/share/licenses/taskforest-i/THIRD-PARTY-NOTICES.txt"
    fi
fi

# Normalize directory and file permissions.
chmod 755 "$work" "$work/usr"
find "$work/usr" -type d -exec chmod 755 {} +
find "$work/usr" -type f -exec chmod a+r {} +

if [[ -n "$stage_only" ]]; then
    mkdir -p "$stage_only"
    cp -a "$work/usr" "$stage_only/"
    echo "build-deb-iced: staged tree written to $stage_only/usr"
    exit 0
fi

# Installed-Size is measured in KiB over the data payload.
installed_size=$(du -sk --exclude=DEBIAN "$work/usr" | cut -f1)
mkdir -p "$work/DEBIAN"
chmod 755 "$work/DEBIAN"

control_src="$script_dir/control-iced"
[[ -f "$control_src" ]] || { echo "build-deb-iced: missing control template $control_src" >&2; exit 1; }

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
    echo "build-deb-iced: neither dpkg-deb nor ar/tar is available to create .deb package" >&2
    exit 1
fi

echo "build-deb-iced: $(basename "$output") ready ($(du -h "$output" | cut -f1))"
