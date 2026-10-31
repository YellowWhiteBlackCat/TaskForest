#!/usr/bin/env bash
# Build the TaskForest AppImage (GPUI frontend only).
#
# The AppImage is the portable, user-space surface: it carries ONLY
# taskforest-g plus the freedesktop entry/icon inside the squashfs. The
# privileged helpers and polkit actions are system-install concerns that cannot
# live in a relocatable image; history persistence remains owned by the running
# frontend process.
#
# linuxdeploy assembles the AppDir and AppRun; it is fetched as a pinned
# upstream AppImage unless LINUXDEPLOY_BIN already points at one. Newer distros
# (and CI images) lack libfuse2, so the tool runs in extract-and-run mode.
#
# The artifact name is NOT the caller's choice: it is derived here from
# docs/PRODUCT_IDENTITY.md (`TaskForest-<UI>-<version>-<platform>.<format>`),
# because the single release-asset convention is what lets users and the
# release notes address one artifact by name. The caller passes the output
# DIRECTORY; the script owns the file name.
#
# Usage:
#   packaging/linux/build-appimage.sh TASKFOREST_G_BIN VERSION OUTPUT_DIR
#   packaging/linux/build-appimage.sh --print-name VERSION [PLATFORM]
#   packaging/linux/build-appimage.sh --self-test
#
# APPIMAGE_PLATFORM overrides the platform label (default: the host machine,
# normalized to the release spelling `x64` / `arm64`).
set -euo pipefail
export LC_ALL=C

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$script_dir/../.." && pwd)
mkdir -p "$repo/.tmp"

# The release platform label (PRODUCT_IDENTITY.md): x64 / arm64, never the
# kernel's `uname -m` spelling and never the DEB/RPM arch spelling.
platform_label() {
    case "${1:-$(uname -m)}" in
        x86_64|amd64|x64) printf 'x64' ;;
        aarch64|arm64) printf 'arm64' ;;
        *)
            echo "build-appimage: unsupported platform '${1:-$(uname -m)}'" >&2
            exit 1
            ;;
    esac
}

# TaskForest-G-<version>-<platform>.AppImage, verbatim from the product
# identity naming rule.
artifact_name() {
    printf 'TaskForest-G-%s-%s.AppImage' "$1" "$2"
}

# Stage the AppDir payload the AppImage carries: the GPUI binary plus the
# freedesktop entry and icon set. linuxdeploy consumes this directory.
stage_appdir() {
    local bin="$1" appdir="$2"
    mkdir -p "$appdir/usr/bin" \
        "$appdir/usr/share/applications" \
        "$appdir/usr/share/icons/hicolor/scalable/apps"
    install -m755 "$bin" "$appdir/usr/bin/taskforest-g"
    install -m644 "$script_dir/io.github.YellowWhiteBlackCat.TaskForestG.desktop" \
        "$appdir/usr/share/applications/"io.github.YellowWhiteBlackCat.TaskForestG.desktop
    install -m644 "$script_dir/io.github.YellowWhiteBlackCat.TaskForest.svg" \
        "$appdir/usr/share/icons/hicolor/scalable/apps/taskforest-taskboard.svg"
    install -m644 "$script_dir/io.github.YellowWhiteBlackCat.TaskForest.svg" \
        "$appdir/usr/share/icons/hicolor/scalable/apps/io.github.YellowWhiteBlackCat.TaskForest.svg"
    local size icon_png
    for size in 16 24 32 48 64 128 256 512; do
        icon_png="$script_dir/icons/hicolor/${size}x${size}/apps/taskforest-taskboard.png"
        if [[ -f "$icon_png" ]]; then
            mkdir -p "$appdir/usr/share/icons/hicolor/${size}x${size}/apps"
            install -m644 "$icon_png" "$appdir/usr/share/icons/hicolor/${size}x${size}/apps/taskforest-taskboard.png"
            install -m644 "$icon_png" "$appdir/usr/share/icons/hicolor/${size}x${size}/apps/io.github.YellowWhiteBlackCat.TaskForest.png"
        fi
    done
}

# Headless self-test: the release name rule and the whole AppDir payload are
# checked WITHOUT linuxdeploy, network, or FUSE, so the quick gate can prove
# the layout and the artifact name on any Linux host.
self_test() {
    local expected name bin ok
    expected='TaskForest-G-0.1.0-rc5-x64.AppImage'
    name="$(artifact_name '0.1.0-rc5' "$(platform_label x86_64)")"
    if [[ "$name" != "$expected" ]]; then
        echo "build-appimage: self-test FAIL — name '$name' != '$expected'" >&2
        exit 1
    fi
    [[ "$(platform_label aarch64)" == 'arm64' ]] \
        || { echo "build-appimage: self-test FAIL — arm64 label" >&2; exit 1; }

    # A script-level variable, never a function local: the EXIT trap runs
    # after the function returns, when a local would already be unbound.
    selftest_dir="$(mktemp -d "$repo/.tmp/build-appimage-selftest.XXXXXX")"
    trap 'rm -rf "$selftest_dir"' EXIT
    bin="$selftest_dir/taskforest-g"
    printf '#!/bin/sh\nexit 0\n' >"$bin"
    chmod +x "$bin"
    stage_appdir "$bin" "$selftest_dir/AppDir"
    ok=1
    for path in \
        usr/bin/taskforest-g \
        usr/share/applications/io.github.YellowWhiteBlackCat.TaskForestG.desktop \
        usr/share/icons/hicolor/scalable/apps/io.github.YellowWhiteBlackCat.TaskForest.svg; do
        [[ -e "$selftest_dir/AppDir/$path" ]] || { echo "build-appimage: self-test FAIL — missing $path" >&2; ok=0; }
    done
    [[ "$ok" -eq 1 ]] || exit 1
    [[ -x "$selftest_dir/AppDir/usr/bin/taskforest-g" ]] \
        || { echo "build-appimage: self-test FAIL — payload is not executable" >&2; exit 1; }
    echo "build-appimage: self-test PASS ($name, AppDir layout verified)"
}

case "${1:-}" in
    --print-name)
        [[ $# -ge 2 ]] || { echo "usage: $0 --print-name VERSION [PLATFORM]" >&2; exit 2; }
        artifact_name "$2" "$(platform_label "${3:-}")"
        exit 0
        ;;
    --self-test)
        self_test
        exit 0
        ;;
esac

if [[ $# -ne 3 ]]; then
    echo "usage: $0 TASKFOREST_G_BIN VERSION OUTPUT_DIR" >&2
    exit 2
fi
bin=$1
version=$2
output_dir=$3

[[ -x "$bin" ]] || { echo "build-appimage: $bin is not executable" >&2; exit 1; }
[[ -f "$script_dir/io.github.YellowWhiteBlackCat.TaskForestG.desktop" ]] \
    || { echo "build-appimage: TaskForestG desktop entry missing" >&2; exit 1; }
[[ -s "$script_dir/io.github.YellowWhiteBlackCat.TaskForest.svg" ]] \
    || { echo "build-appimage: TaskForest icon is missing or empty" >&2; exit 1; }

mkdir -p "$output_dir"
output_dir=$(cd "$output_dir" && pwd)
output="$output_dir/$(artifact_name "$version" "$(platform_label "${APPIMAGE_PLATFORM:-}")")"

work=$(mktemp -d "$repo/.tmp/build-appimage.XXXXXX")
trap 'rm -rf "$work"' EXIT
appdir="$work/AppDir"
stage_appdir "$bin" "$appdir"

linuxdeploy_bin=${LINUXDEPLOY_BIN:-$work/linuxdeploy-x86_64.AppImage}
if [[ ! -x "$linuxdeploy_bin" ]]; then
    curl -fsSL --retry 3 \
        -o "$linuxdeploy_bin" \
        https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage
    chmod +x "$linuxdeploy_bin"
fi

# APPIMAGE_EXTRACT_AND_RUN sidesteps the libfuse2 dependency on modern hosts.
# OUTPUT pins the artifact name linuxdeploy emits for the appimage target.
env APPIMAGE_EXTRACT_AND_RUN=1 ARCH=x86_64 OUTPUT="$output" \
    "$linuxdeploy_bin" \
    --appdir "$appdir" \
    -e "$appdir/usr/bin/taskforest-g" \
    -d "$appdir/usr/share/applications/io.github.YellowWhiteBlackCat.TaskForestG.desktop" \
    -i "$appdir/usr/share/icons/hicolor/scalable/apps/taskforest-taskboard.svg" \
    --output appimage >/dev/null

[[ -f "$output" ]] || { echo "build-appimage: $output was not produced" >&2; exit 1; }
echo "build-appimage: $(basename "$output") ready ($(du -h "$output" | cut -f1))"
