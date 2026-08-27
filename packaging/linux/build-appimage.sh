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
# Usage: packaging/linux/build-appimage.sh TASKFOREST_G_BIN VERSION OUTPUT_APPIMAGE
set -euo pipefail
export LC_ALL=C

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$script_dir/../.." && pwd)
mkdir -p "$repo/.tmp"

if [[ $# -ne 3 ]]; then
    echo "usage: $0 TASKFOREST_G_BIN VERSION OUTPUT_APPIMAGE" >&2
    exit 2
fi
bin=$1
version=$2
output=$3

[[ -x "$bin" ]] || { echo "build-appimage: $bin is not executable" >&2; exit 1; }
[[ -f "$script_dir/io.github.YellowWhiteBlackCat.TaskForestG.desktop" ]] \
    || { echo "build-appimage: TaskForestG desktop entry missing" >&2; exit 1; }
[[ -s "$script_dir/io.github.YellowWhiteBlackCat.TaskForest.svg" ]] \
    || { echo "build-appimage: TaskForest icon is missing or empty" >&2; exit 1; }

work=$(mktemp -d "$repo/.tmp/build-appimage.XXXXXX")
trap 'rm -rf "$work"' EXIT
appdir="$work/AppDir"
mkdir -p "$appdir/usr/bin" \
    "$appdir/usr/share/applications" \
    "$appdir/usr/share/icons/hicolor/scalable/apps"
install -m755 "$bin" "$appdir/usr/bin/taskforest-g"
install -m644 "$script_dir/io.github.YellowWhiteBlackCat.TaskForestG.desktop" \
    "$appdir/usr/share/applications/"
install -m644 "$script_dir/io.github.YellowWhiteBlackCat.TaskForest.svg" \
    "$appdir/usr/share/icons/hicolor/scalable/apps/taskforest-taskboard.svg"

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
