#!/usr/bin/env bash
# Build and bundle the TaskForest Flatpak package.
#
# Usage:
#   packaging/flatpak/build-flatpak.sh [OUTPUT_BUNDLE]
set -euo pipefail
export LC_ALL=C

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$script_dir/../.." && pwd)
mkdir -p "$repo/.tmp"

output=${1:-"$repo/TaskForest.flatpak"}
manifest="$script_dir/io.github.YellowWhiteBlackCat.TaskForest.json"

if ! command -v flatpak-builder >/dev/null 2>&1; then
    echo "build-flatpak: flatpak-builder is not installed; validating manifest syntax only" >&2
    python3 -c "import json; json.load(open('$manifest'))"
    echo "build-flatpak: manifest syntax valid"
    exit 0
fi

work=$(mktemp -d "$repo/.tmp/build-flatpak.XXXXXX")
trap 'rm -rf "$work"' EXIT

build_dir="$work/build"
repo_dir="$work/repo"

flatpak-builder --force-clean --repo="$repo_dir" "$build_dir" "$manifest"
flatpak build-bundle "$repo_dir" "$output" "io.github.YellowWhiteBlackCat.TaskForest"
echo "build-flatpak: $(basename "$output") ready ($(du -h "$output" | cut -f1))"
