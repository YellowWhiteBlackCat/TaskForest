#!/usr/bin/env bash
# Rebuild every shipped TaskForest application and tray icon from the two
# surface-owned SVG sources.
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
TRAY_SOURCE="$REPO_ROOT/packaging/tray/taskforest-tray.svg"
MACOS_DIR="$REPO_ROOT/packaging/macos"
WINDOWS_ICON="$REPO_ROOT/packaging/windows/taskmanager.ico"
TRAY_RGBA="$REPO_ROOT/crates/taskmanager-assets/assets/product/taskforest-tray-22.rgba"
TRAY_PNG="$TMPDIR/taskforest-tray-22.png"

for command in cargo magick rsvg-convert; do
    command -v "$command" >/dev/null 2>&1 || {
        printf 'required icon tool is unavailable: %s\n' "$command" >&2
        exit 2
    }
done

printf '%s\n' '==> rasterizing TaskForest SVG to the macOS size ladder'
for size in 16 32 64 128 256 512 1024; do
    rsvg-convert -w "$size" -h "$size" "$SOURCE" -o "$MACOS_DIR/icon_${size}.png"
done

printf '%s\n' '==> assembling macOS ICNS with the checked-in Rust tool'
cargo run --locked -j 4 --manifest-path "$MACOS_DIR/build-icns/Cargo.toml"

printf '%s\n' '==> assembling the Windows multi-resolution ICO'
magick "$MACOS_DIR/icon_1024.png" \
    -define icon:auto-resize=16,24,32,48,64,128,256 \
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
