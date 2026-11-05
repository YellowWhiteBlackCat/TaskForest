#!/usr/bin/env bash
# Rebuild the seamless neutral grain consumed by Bevy's glass material.
#
# The tile is deterministic and repository-owned: a fixed integer hash produces
# near-white luminance with a low-alpha grain. Bevy repeats the checked-in RGBA
# plate through NodeImageMode::Tiled, so the material never depends on a runtime
# random source or a new image-decoding package.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

for command in python3; do
    command -v "$command" >/dev/null 2>&1 || {
        printf 'required glass-noise tool is unavailable: %s\n' "$command" >&2
        exit 2
    }
done

python3 - "$REPO_ROOT/crates/taskmanager-assets/assets/glass/noise-64.rgba" <<'PY'
from pathlib import Path
import sys

size = 64
destination = Path(sys.argv[1])
destination.parent.mkdir(parents=True, exist_ok=True)
pixels = bytearray()


def value(x: int, y: int) -> int:
    mixed = (x * 1103515245 + y * 12345 + 0x9E3779B9) & 0xFFFFFFFF
    mixed ^= mixed >> 16
    mixed = (mixed * 0x85EBCA6B) & 0xFFFFFFFF
    mixed ^= mixed >> 13
    mixed = (mixed * 0xC2B2AE35) & 0xFFFFFFFF
    mixed ^= mixed >> 16
    return mixed


for y in range(size):
    for x in range(size):
        grain = value(x, y)
        channel = 244 + grain % 12
        alpha = 255
        pixels.extend((channel, channel, channel, alpha))

destination.write_bytes(pixels)
expected = size * size * 4
actual = destination.stat().st_size
if actual != expected:
    raise SystemExit(f"{destination}: expected {expected} bytes, got {actual}")
print(f"{destination.relative_to(Path.cwd())} ok")
PY

printf 'Glass noise tile regenerated.\n'
