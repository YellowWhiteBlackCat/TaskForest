#!/usr/bin/env bash
# Capture a real taskforest-i Windows window and a matched GPUI baseline.
# Run from Git Bash; Windows.Graphics.Capture binds to the app-emitted PID.

set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"

kernel="$(uname -s)"
case "$kernel" in
MINGW* | MSYS* | CYGWIN*) ;;
*)
    echo "capture-iced-windows.sh is Windows-only (uname: $kernel)" >&2
    exit 2
    ;;
esac

requested_size="${TM_CAPTURE_WINDOW_SIZE:-1180x780}"
requested_theme="${TM_SKIN:-windows-dark}"
high_contrast="${TM_SKIN_HC:-0}"
scenario="${TM_CAPTURE_SCENARIO:-performance-cpu}"
pair_id="${TM_CAPTURE_PAIR_ID:-}"
out_root=""

while [[ $# -gt 0 ]]; do
    case "$1" in
    --pair-id)
        shift
        pair_id="${1:-}"
        ;;
    --size)
        shift
        requested_size="${1:-}"
        ;;
    --theme)
        shift
        requested_theme="${1:-}"
        ;;
    --out)
        shift
        out_root="${1:-}"
        ;;
    *)
        echo "unknown argument '$1'" >&2
        exit 2
        ;;
    esac
    shift
done

case "$requested_theme" in
windows-dark | windows-light) ;;
*) requested_theme=windows-dark ;;
esac
case "$high_contrast" in
0 | 1) ;;
*)
    echo "TM_SKIN_HC must be 0 or 1" >&2
    exit 2
    ;;
esac
if [ "$requested_size" != "1180x780" ]; then
    echo "paired Windows evidence requires requested_window_size=1180x780" >&2
    exit 2
fi
if [ "$scenario" != "performance-cpu" ]; then
    echo "this Windows pair currently supports scenario=performance-cpu" >&2
    exit 2
fi
if [ -z "$pair_id" ]; then
    pair_id="CORE-07-${scenario}-${requested_theme}-${requested_size}"
fi
if [[ ! "$pair_id" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{2,127}$ ]]; then
    echo "pair id contains unsupported characters" >&2
    exit 2
fi

for command in cargo cygpath git rustc sha256sum timeout; do
    if ! command -v "$command" >/dev/null 2>&1; then
        echo "required command is unavailable: $command" >&2
        exit 2
    fi
done
if ! command -v python3 >/dev/null 2>&1; then
    echo "required Python interpreter is unavailable" >&2
    exit 2
fi

export CARGO_BUILD_JOBS=4
export TM_SKIN="$requested_theme"
export TM_SKIN_HC="$high_contrast"
export TM_CAPTURE_SCENARIO="$scenario"
export TM_CAPTURE_PAIR_ID="$pair_id"
export TM_CAPTURE_WINDOW_SIZE="$requested_size"

stamp="$(date -u +%Y%m%dT%H%M%SZ)"
git_head="$(git rev-parse --short=12 HEAD)"
if [ -n "$(git status --porcelain)" ]; then
    worktree=dirty
else
    worktree=clean
fi
rust="$(rustc -V)"
if [ -z "$out_root" ]; then
    out_root="target/windows-evidence/${pair_id}/${stamp}_${git_head}_${worktree}"
fi
mkdir -p "$out_root"
iced_dir="$out_root/iced"
gpui_dir="$out_root/gpui"
mkdir -p "$iced_dir"
if [ -e "$gpui_dir/capture.png" ] || [ -e "$iced_dir/capture.png" ]; then
    echo "capture output already exists under $out_root; choose a new --out directory" >&2
    exit 2
fi

echo "=== capture-iced-windows: pair=$pair_id ($git_head, $worktree) ==="

# Bind this run's source graph before either binary launches. The validator
# recomputes every listed digest after capture, so a dirty-tree edit during the
# paired run fails closed even when the coarse worktree state stays 'dirty'.
timeout --kill-after=5s 60s python3 scripts/frontend_source_manifest.py \
    --frontend iced --repo-root "$repo" --output "$iced_dir/source-manifest.tsv"

echo "building taskforest-i.exe (Iced product) ..."
cargo build --locked -j 4 -p taskmanager-iced --bin taskforest-i
iced_binary="target/debug/taskforest-i.exe"
if [ ! -f "$iced_binary" ]; then
    echo "missing $iced_binary after the Iced product build" >&2
    exit 2
fi

# Build and capture the GPUI side with the identical page, device, viewport,
# theme, contrast, and pair identity. The shared GPUI runner owns its own
# WGC target binding and receipt validator.
TM_PAGE=performance \
TM_DEVICE=cpu \
TM_WINDOW_SIZE="$requested_size" \
TM_SKIN="$requested_theme" \
TM_SKIN_HC="$high_contrast" \
TM_CAPTURE_SCENARIO="$scenario" \
TM_CAPTURE_PAIR_ID="$pair_id" \
bash scripts/capture-windows.sh --pair-id "$pair_id" --out "$gpui_dir"

for peer_file in receipt-metadata.txt capture-metadata.txt source-manifest.tsv capture.png; do
    if [ ! -s "$gpui_dir/$peer_file" ]; then
        echo "GPUI baseline is missing $peer_file" >&2
        exit 2
    fi
done

app_marker="$iced_dir/app-marker.txt"
app_log="$iced_dir/app.log"
app_marker_win="$(cygpath -aw "$app_marker")"
iced_dir_win="$(cygpath -aw "$iced_dir")"
rm -f "$app_marker" "$app_log"
TM_ICED_CAPTURE_DEVICE=cpu \
TM_ICED_WINDOW_SIZE="$requested_size" \
TM_ICED_CAPTURE_LOCALE=en \
TM_ICED_CAPTURE_MARKER_FILE="$app_marker_win" \
"$iced_binary" --demo >"$app_log" 2>&1 &
app_pid=$!

stop_app() {
    if [ -n "${app_pid:-}" ]; then
        kill "$app_pid" 2>/dev/null || true
        for _ in {1..20}; do
            kill -0 "$app_pid" 2>/dev/null || break
            sleep 0.1
        done
        kill -KILL "$app_pid" 2>/dev/null || true
        wait "$app_pid" 2>/dev/null || true
        app_pid=""
    fi
}
trap stop_app EXIT INT TERM

marker_ready=0
for _ in {1..160}; do
    if [ -f "$app_marker" ] \
        && grep -Fqx 'ICED_CAPTURE_MARKER event=frame_ready mode=demo page=performance' "$app_marker" \
        && grep -Fqx 'ICED_CAPTURE_MARKER event=target_ready mode=demo page=performance device=cpu' "$app_marker" \
        && grep -Eq '^ICED_CAPTURE_MARKER event=process_ready pid=[1-9][0-9]*$' "$app_marker"; then
        marker_ready=1
        break
    fi
    if ! kill -0 "$app_pid" 2>/dev/null; then
        break
    fi
    sleep 0.25
done
if [ "$marker_ready" != "1" ]; then
    echo "Iced app did not emit its Performance/CPU/PID markers" >&2
    cat "$app_log" >&2 || true
    exit 2
fi

target_pid="$(sed -n 's/^ICED_CAPTURE_MARKER event=process_ready pid=\([1-9][0-9]*\)$/\1/p' "$app_marker")"
if [ "$(printf '%s\n' "$target_pid" | wc -l | tr -d '[:space:]')" != "1" ]; then
    echo "Iced app emitted a missing or duplicate process PID marker" >&2
    exit 2
fi
sleep 1

TM_ICED_CAPTURE_TARGET_PID="$target_pid" \
timeout --kill-after=10s 60 "$iced_binary" --capture-window "$iced_dir_win"
stop_app
trap - EXIT INT TERM

for artifact in capture.png capture-target.txt app-marker.txt; do
    if [ ! -s "$iced_dir/$artifact" ]; then
        echo "Iced Windows capture is missing $artifact" >&2
        exit 2
    fi
done

head_after="$(git rev-parse --short=12 HEAD)"
if [ -n "$(git status --porcelain)" ]; then
    worktree_after=dirty
else
    worktree_after=clean
fi
rust_after="$(rustc -V)"
if [ "$head_after" != "$git_head" ] || [ "$worktree_after" != "$worktree" ] || [ "$rust_after" != "$rust" ]; then
    echo "repository provenance changed during the paired capture" >&2
    exit 2
fi

timeout --kill-after=5s 60s python3 - "$repo" "$iced_dir" "$pair_id" "$scenario" "$requested_theme" \
    "$high_contrast" "$requested_size" "$git_head" "$worktree" "$rust" <<'PY'
from __future__ import annotations

import hashlib
import sys
from datetime import datetime, timezone
from pathlib import Path

repo = Path(sys.argv[1]).resolve()
iced_dir = Path(sys.argv[2]).resolve()
pair_id, scenario, theme, high_contrast, requested_size, git_head, worktree, rust = sys.argv[3:]
sys.path.insert(0, str(repo / "scripts"))
from validate_iced_windows_capture import parse_metadata, resolved_renderer_text_stack, sha256_file

target = parse_metadata(iced_dir / "capture-target.txt")
peer_receipt = parse_metadata(iced_dir.parent / "gpui" / "receipt-metadata.txt")
peer_capture = parse_metadata(iced_dir.parent / "gpui" / "capture-metadata.txt")
renderer_stack, text_stack = resolved_renderer_text_stack(repo / "Cargo.lock")

values = {
    "frontend": "iced",
    "target": "windows",
    "scenario": scenario,
    "pair_id": pair_id,
    "theme": theme,
    "high_contrast": high_contrast,
    "requested_window_size": requested_size,
    "window_size": target["window_size"],
    "window_rect_size": target["window_rect_size"],
    "capture_api": target["capture_api"],
    "window_title": target["window_title"],
    "window_process_name": target["window_process_name"],
    "target_pid": target["target_pid"],
    "binary": "target/debug/taskforest-i.exe",
    "binary_sha256": sha256_file(repo / "target/debug/taskforest-i.exe"),
    "git_head": git_head,
    "worktree": worktree,
    "rust": rust,
    "cargo_lock_sha256": sha256_file(repo / "Cargo.lock"),
    "renderer_stack": renderer_stack,
    "renderer_active": target["renderer_active"],
    "text_stack": text_stack,
    "text_shaping": target["text_shaping"],
    "text_rasterizer": target["text_rasterizer"],
    "text_rasterizer_active": target["text_rasterizer_active"],
    "native_text_api": target["native_text_api"],
    "font_weight_axis_policy": target["font_weight_axis_policy"],
    "font_weight_450_name": target["font_weight_450_name"],
    "font_weight_450_value": target["font_weight_450_value"],
    "source_scope": "iced",
    "source_manifest": "source-manifest.tsv",
    "source_manifest_sha256": sha256_file(iced_dir / "source-manifest.tsv"),
    "marker_file": "app-marker.txt",
    "marker_sha256": sha256_file(iced_dir / "app-marker.txt"),
    "target_metadata": "capture-target.txt",
    "target_metadata_sha256": sha256_file(iced_dir / "capture-target.txt"),
    "peer_frontend": peer_receipt["frontend"],
    "peer_target": peer_receipt["target"],
    "peer_scenario": peer_receipt["scenario"],
    "peer_theme": peer_receipt["theme"],
    "peer_high_contrast": peer_receipt["high_contrast"],
    "peer_pair_id": peer_receipt["pair_id"],
    "peer_requested_window_size": peer_receipt["requested_window_size"],
    "peer_window_size": peer_capture["window_size"],
    "peer_png_sha256": peer_capture["png_sha256"],
    "peer_binary_sha256": peer_receipt["binary_sha256"],
    "peer_source_manifest_sha256": peer_receipt["source_manifest_sha256"],
    "peer_receipt": "../gpui/receipt-metadata.txt",
    "peer_capture_metadata": "../gpui/capture-metadata.txt",
    "peer_image": "../gpui/capture.png",
    "peer_source_manifest": "../gpui/source-manifest.tsv",
    "timestamp_utc": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "png_sha256": sha256_file(iced_dir / "capture.png"),
}
metadata = "".join(f"{key}={value}\n" for key, value in values.items())
(iced_dir / "capture-metadata.txt").write_text(metadata, encoding="utf-8")
PY

timeout --kill-after=5s 60s python3 scripts/validate_iced_windows_capture.py \
    --bundle "$iced_dir" --repo-root "$repo"
cp "$iced_dir/capture-metadata.txt" "$iced_dir/receipt-metadata.txt"

echo "capture-iced-windows: OK"
echo "iced_receipt=$iced_dir/receipt-metadata.txt"
echo "gpui_receipt=$gpui_dir/receipt-metadata.txt"
