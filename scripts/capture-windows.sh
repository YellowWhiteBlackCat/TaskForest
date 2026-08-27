#!/usr/bin/env bash
# capture-windows.sh — Windows native pixel-evidence runner (Git Bash only).
#
# The Windows+GPUI product owns its real window; this script drives the
# in-process self-capture mode (`taskmanager --capture-window <dir>`) which
# opens the app, waits for paint, grabs its own Windows.Graphics.Capture frame
# once, and writes capture.png + capture-metadata.txt + capture-manifest.tsv
# before exiting 0.
#
# The script then validates the evidence (PNG signature, non-blank content,
# sha256 vs metadata) and writes a receipt-metadata.txt with git head /
# worktree state — mirroring the Linux capture-flow evidence discipline. The
# pixel REVIEW (visual inspection) is a separate human step, recorded through
# the quality-gate workflow; this runner only guarantees the pixels are real and
# self-consistent.
#
# Usage:
#   bash scripts/capture-windows.sh [--skip-build] [--out <dir>]
#   Default out: target/windows-evidence/<stamp>_<head>_<state>/
#
# PowerShell is forbidden everywhere in this repository (automation included).

set -u

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"

kernel="$(uname -s)"
case "$kernel" in
MINGW* | MSYS* | CYGWIN*) ;;
*)
    echo "capture-windows.sh is Windows-only (uname: $kernel); on Linux use scripts/capture-niri.sh" >&2
    exit 2
    ;;
esac

skip_build=0
out_dir=""
while [[ $# -gt 0 ]]; do
    case "$1" in
    --skip-build) skip_build=1 ;;
    --out)
        shift
        out_dir="${1:-}"
        ;;
    *)
        echo "unknown argument '$1'" >&2
        exit 2
        ;;
    esac
    shift
done

if ! timeout 5s python3 --version >/dev/null 2>&1; then
    echo "Python 3 interpreter is unavailable" >&2
    exit 2
fi
for command in cargo git sha256sum timeout; do
    if ! command -v "$command" >/dev/null 2>&1; then
        echo "required command is unavailable: $command" >&2
        exit 2
    fi
done

CARGO_BUILD_JOBS="${JOBS:-4}"
export CARGO_BUILD_JOBS

stamp="$(date -u +%Y%m%dT%H%M%SZ)"
head="$(git rev-parse --short=12 HEAD 2>/dev/null || echo no-git)"
if [ -n "$(git status --porcelain)" ]; then
    worktree=dirty
else
    worktree=clean
fi
if [ -z "$out_dir" ]; then
    out_dir="target/windows-evidence/${stamp}_${head}_${worktree}"
fi
mkdir -p "$out_dir"

echo "=== capture-windows: $stamp ($head, $worktree) ==="

if [ "$skip_build" != "1" ]; then
    echo "building taskforest-g.exe (ui-gpui) ..."
    cargo build -j 4 || exit 2
fi
binary="target/debug/taskforest-g.exe"
if [ "$skip_build" != "1" ]; then
    cp -f target/debug/taskmanager.exe "$binary"
fi
if [ ! -x "$binary" ]; then
    echo "missing $binary; build the ui-gpui shape first (or pass --skip-build after a named build)" >&2
    exit 2
fi

# The app opens a real window on the desktop and exits after the capture.
timeout --kill-after=10s 60 "$binary" --capture-window "$out_dir"
app_rc=$?
if [ "$app_rc" -ne 0 ]; then
    echo "capture-windows: the app exited $app_rc" >&2
    exit 2
fi

png="$out_dir/capture.png"
metadata="$out_dir/capture-metadata.txt"
[ -f "$png" ] || { echo "capture-windows: missing $png" >&2; exit 2; }
[ -f "$metadata" ] || { echo "capture-windows: missing $metadata" >&2; exit 2; }

# PNG signature + non-blank content + sha256 consistency.
timeout --kill-after=10s 60 python3 - "$out_dir" <<'PY'
import hashlib
import struct
import sys
import zlib

out = sys.argv[1]
png_path = out + "/capture.png"
data = open(png_path, "rb").read()
assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG signature"
pos, idat, w = 8, b"", 0
while pos < len(data):
    length, typ = struct.unpack(">I4s", data[pos:pos + 8])
    chunk = data[pos + 8:pos + 8 + length]
    if typ == b"IHDR":
        w = struct.unpack(">I", chunk[:4])[0]
    elif typ == b"IDAT":
        idat += chunk
    pos += 12 + length
assert w > 0, "empty IHDR width"
raw = zlib.decompress(idat)
# Coarse color-diversity sample to prove the frame is not blank.
stride = w * 3
sample = set()
step = max(1, w // 64)
for y in range(0, len(raw) // stride, max(1, (len(raw) // stride) // 64)):
    row = raw[y * stride:(y + 1) * stride]
    for x in range(0, w, step):
        sample.add(row[x * 3:(x + 1) * 3])
assert len(sample) > 20, "frame appears blank (too few distinct colors)"
meta = open(out + "/capture-metadata.txt", encoding="utf-8").read()
expected = hashlib.sha256(data).hexdigest()
line = next(l for l in meta.splitlines() if l.startswith("png_sha256="))
assert line.split("=", 1)[1] == expected, "png_sha256 mismatch"
print(f"validate: OK png={w}x{(len(raw)//stride)} sha256={expected[:16]}... colors={len(sample)}")
PY
validate_rc=$?
if [ "$validate_rc" -ne 0 ]; then
    echo "capture-windows: evidence validation failed" >&2
    exit 2
fi

cat >"$out_dir/receipt-metadata.txt" <<EOF
command=bash scripts/capture-windows.sh
timestamp=$stamp
git_head=$head
worktree=$worktree
app_exit=$app_rc
host=$(hostname 2>/dev/null || echo unknown)
capture_api=windows.graphics.capture (windows-capture)
EOF

echo "capture-windows: OK"
echo "evidence=$out_dir"
