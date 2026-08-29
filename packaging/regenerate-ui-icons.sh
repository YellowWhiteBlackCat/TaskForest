#!/usr/bin/env bash
# Rebuild the shared UI icon bitmaps consumed by the bevy_ui frontend.
#
# GPUI rasterizes the embedded SVGs at layout time; bevy_ui has no runtime SVG
# path, so it draws the SAME semantic icons (taskmanager-icons::path) from
# checked-in white RGBA bitmaps tinted at draw time. This script keeps those
# bitmaps derived from the one shared SVG source set — a hand-edited
# derivative is as forbidden here as anywhere else in the repository.
#
# Bitmaps rasterize at UI_ICON_RGBA_SIZE (2x of the 18px logical maximum) so
# 1x-scale drawing downsamples crisply and 2x-scale drawing is exact.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"
SIZE=36
ASSET_DIR="$REPO_ROOT/crates/taskmanager-assets/assets/icons-rgba"

for command in rsvg-convert magick; do
    command -v "$command" >/dev/null 2>&1 || {
        printf 'required icon tool is unavailable: %s\n' "$command" >&2
        exit 2
    }
done

# (source svg path, flattened output name) — keys are the taskmanager-icons
# asset paths so the Rust lookup table can reuse `icon_path` constants and
# the generic `icons/*.svg` literals verbatim.
ICONS=(
    "domain/cpu.svg domain-cpu"
    "domain/memory.svg domain-memory"
    "domain/disk.svg domain-disk"
    "domain/network.svg domain-network"
    "domain/gpu.svg domain-gpu"
    "domain/process.svg domain-process"
    "domain/service.svg domain-service"
    "domain/startup.svg domain-startup"
    "domain/user.svg domain-user"
    "domain/alert.svg domain-alert"
    "domain/search.svg domain-search"
    "domain/settings.svg domain-settings"
    "icons/chart-pie.svg icons-chart-pie"
    "icons/info.svg icons-info"
    "icons/layout-dashboard.svg icons-layout-dashboard"
    "icons/arrow-up.svg icons-arrow-up"
    "icons/arrow-down.svg icons-arrow-down"
)

mkdir -p "$ASSET_DIR"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

for entry in "${ICONS[@]}"; do
    read -r source name <<<"$entry"
    svg="$REPO_ROOT/crates/taskmanager-assets/assets/$source"
    [[ -s "$svg" ]] || { printf 'missing icon source: %s\n' "$svg" >&2; exit 1; }
    # The lucide sources stroke with `currentColor`; force white so the tint
    # at draw time is a clean multiply against the theme ink.
    sed 's/currentColor/#ffffff/g' "$svg" >"$TMP/icon.svg"
    rsvg-convert -w "$SIZE" -h "$SIZE" "$TMP/icon.svg" -o "$TMP/icon.png"
    magick "$TMP/icon.png" -depth 8 "RGBA:$ASSET_DIR/$name.rgba"
    expected=$((SIZE * SIZE * 4))
    actual="$(wc -c <"$ASSET_DIR/$name.rgba")"
    [[ "$actual" -eq "$expected" ]] || {
        printf '%s.rgba: expected %s bytes, got %s\n' "$name" "$expected" "$actual" >&2
        exit 1
    }
    printf '%s.rgba ok\n' "$name"
done

printf 'UI icon bitmaps regenerated from the shared SVG sources.\n'
