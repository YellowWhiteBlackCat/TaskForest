#!/usr/bin/env bash
# Rebuild every shipped TaskForest application and tray icon from the
# surface-owned SVG masters, including the pixel-aligned 16px reduction.
#
# The application SVG remains at the historical Linux packaging path so
# installed desktop identities stay upgrade-compatible. The tray SVG is an
# authored optical-size reduction of the same brand, not a platform variant.
# No platform or frontend may carry a hand-edited derivative.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"
eval "$(scripts/agent-workdir.sh enter icon-regeneration)"
export CARGO_BUILD_JOBS=4
export RUSTC_WRAPPER=

SOURCE="$REPO_ROOT/packaging/linux/io.github.YellowWhiteBlackCat.TaskForest.svg"
SMALL_SOURCE="$REPO_ROOT/crates/taskmanager-assets/assets/product/taskforest-logo-small.svg"
EMBEDDED_SOURCE="$REPO_ROOT/crates/taskmanager-assets/assets/product/taskforest-logo.svg"
TRAY_SOURCE="$REPO_ROOT/packaging/tray/taskforest-tray.svg"
MACOS_DIR="$REPO_ROOT/packaging/macos"
LINUX_ICON_DIR="$REPO_ROOT/packaging/linux/icons/hicolor"
WINDOWS_ICON="$REPO_ROOT/packaging/windows/taskmanager.ico"
TRAY_RGBA="$REPO_ROOT/crates/taskmanager-assets/assets/product/taskforest-tray-22.rgba"
TRAY_PNG="$TMPDIR/taskforest-tray-22.png"

for command in cargo magick rsvg-convert; do
    command -v "$command" >/dev/null 2>&1 || {
        printf 'required icon tool is unavailable: %s\n' "$command" >&2
        exit 2
    }
done

printf '%s\n' '==> synchronizing the embedded product SVG'
cp -f "$SOURCE" "$EMBEDDED_SOURCE"

printf '%s\n' '==> rasterizing TaskForest SVG to the macOS size ladder'
for size in 16 32 64 128 256 512 1024; do
    optical_source="$SOURCE"
    if [[ "$size" -eq 16 ]]; then optical_source="$SMALL_SOURCE"; fi
    rsvg-convert -w "$size" -h "$size" "$optical_source" -o "$MACOS_DIR/icon_${size}.png"
done

printf '%s\n' '==> rasterizing the freedesktop hicolor size ladder'
for size in 16 24 32 48 64 128 256 512; do
    destination="$LINUX_ICON_DIR/${size}x${size}/apps/taskforest-taskboard.png"
    destination_canonical="$LINUX_ICON_DIR/${size}x${size}/apps/io.github.YellowWhiteBlackCat.TaskForest.png"
    mkdir -p "$(dirname "$destination")"
    optical_source="$SOURCE"
    if [[ "$size" -eq 16 ]]; then optical_source="$SMALL_SOURCE"; fi
    rsvg-convert -w "$size" -h "$size" "$optical_source" -o "$destination"
    cp -f "$destination" "$destination_canonical"
done
mkdir -p "$LINUX_ICON_DIR/scalable/apps"
cp -f "$SOURCE" "$LINUX_ICON_DIR/scalable/apps/taskforest-taskboard.svg"
cp -f "$SOURCE" "$LINUX_ICON_DIR/scalable/apps/io.github.YellowWhiteBlackCat.TaskForest.svg"

printf '%s\n' '==> assembling macOS ICNS with the checked-in Rust tool'
cargo run --locked -j 4 --manifest-path "$MACOS_DIR/build-icns/Cargo.toml"

printf '%s\n' '==> assembling the Windows multi-resolution ICO'
magick \
    "$MACOS_DIR/icon_16.png" \
    "$LINUX_ICON_DIR/24x24/apps/taskforest-taskboard.png" \
    "$MACOS_DIR/icon_32.png" \
    "$LINUX_ICON_DIR/48x48/apps/taskforest-taskboard.png" \
    "$MACOS_DIR/icon_64.png" \
    "$MACOS_DIR/icon_128.png" \
    "$MACOS_DIR/icon_256.png" \
    "$WINDOWS_ICON"

printf '%s\n' '==> rasterizing the shared 22px native tray icon'
mkdir -p "$(dirname "$TRAY_RGBA")"
rsvg-convert -w 22 -h 22 "$TRAY_SOURCE" -o "$TRAY_PNG"
magick "$TRAY_PNG" -depth 8 "RGBA:$TRAY_RGBA"
[[ "$(wc -c <"$TRAY_RGBA")" -eq $((22 * 22 * 4)) ]] || {
    printf 'unexpected TaskForest tray RGBA byte length\n' >&2
    exit 1
}

file "$MACOS_DIR/icon.icns" "$WINDOWS_ICON" "$TRAY_RGBA"
printf '%s\n' 'TaskForest icons regenerated from the canonical SVG sources.'
