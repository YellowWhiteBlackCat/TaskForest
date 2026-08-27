#!/bin/bash
# Capture the real Ratatui/Crossterm frame inside Alacritty on nested Niri.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"
eval "$(scripts/agent-workdir.sh enter tui-capture)"
# The shared sccache socket exceeds Linux SUN_LEN for this repository path;
# keep Cargo on the shared target but use rustc directly for this bounded run.
export RUSTC_WRAPPER=
# ADR-029: single binary — the TUI shape is `taskmanager` built with
# --no-default-features --features ui-tui.
APP="$REPO/target/debug/taskmanager"
OUT="$REPO/target/tui-evidence/latest"
EVIDENCE_ROOT="$REPO/target/tui-evidence"
CAPTURE_PAGE="${TM_TUI_CAPTURE_PAGE:-performance}"
CAPTURE_DEVICE="${TM_TUI_CAPTURE_DEVICE:-}"
CAPTURE_SCENE="${TM_TUI_CAPTURE_SCENE:-}"
CAPTURE_COLUMNS="${TM_TUI_CAPTURE_COLUMNS:-120}"
CAPTURE_LINES="${TM_TUI_CAPTURE_LINES:-36}"
CAPTURE_FONT_SIZE="${TM_TUI_CAPTURE_FONT_SIZE:-13}"
if ! [[ "$CAPTURE_COLUMNS" =~ ^[0-9]+$ ]] || [ "$CAPTURE_COLUMNS" -lt 54 ]; then
  printf 'TM_TUI_CAPTURE_COLUMNS must be an integer >= 54\n' >&2
  exit 2
fi
if ! [[ "$CAPTURE_LINES" =~ ^[0-9]+$ ]] || [ "$CAPTURE_LINES" -lt 16 ]; then
  printf 'TM_TUI_CAPTURE_LINES must be an integer >= 16\n' >&2
  exit 2
fi
if ! [[ "$CAPTURE_FONT_SIZE" =~ ^[0-9]+$ ]] || [ "$CAPTURE_FONT_SIZE" -lt 8 ] || [ "$CAPTURE_FONT_SIZE" -gt 30 ]; then
  printf 'TM_TUI_CAPTURE_FONT_SIZE must be an integer from 8 to 30\n' >&2
  exit 2
fi
case "$CAPTURE_DEVICE" in
  ""|cpu|memory|disk|network|gpu|battery|fan) ;;
  *)
    printf 'unsupported TM_TUI_CAPTURE_DEVICE=%s\n' "$CAPTURE_DEVICE" >&2
    exit 2
    ;;
esac
case "$CAPTURE_SCENE" in
  "") ;;
  system-npu) CAPTURE_PAGE=system ;;
  *)
    printf 'unsupported TM_TUI_CAPTURE_SCENE=%s\n' "$CAPTURE_SCENE" >&2
    exit 2
    ;;
esac
case "$CAPTURE_PAGE" in
  performance|applications|services|system|startup|users|app-history) ;;
  *)
    printf 'unsupported TM_TUI_CAPTURE_PAGE=%s\n' "$CAPTURE_PAGE" >&2
    exit 2
    ;;
esac
RUN_STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
GIT_HEAD="$(git -C "$REPO" rev-parse --short=12 HEAD 2>/dev/null || printf 'no-git')"
if [ -n "$(git -C "$REPO" status --porcelain 2>/dev/null)" ]; then
  WORKTREE_STATE=dirty
else
  WORKTREE_STATE=clean
fi
RUN_ID="${RUN_STAMP}_${GIT_HEAD}_${WORKTREE_STATE}"
RUN_DIR="$EVIDENCE_ROOT/$RUN_ID"
RUNTIME_DIR="$(mktemp -d /tmp/taskmanager-tui-niri.XXXXXX)"
CONF="$RUN_DIR/config.kdl"
MARKERS="$RUN_DIR/tui-capture-markers.log"
METADATA="$RUN_DIR/tui-capture-metadata.txt"
MANIFEST="$RUN_DIR/tui-capture-manifest.tsv"
RECEIPT="$RUN_DIR/tui-capture-validation.json"
IMAGE="$RUN_DIR/tui-mvp.png"
SOURCE_MANIFEST="$RUN_DIR/tui-source-manifest.sha256"
HOST_XDG="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
HOST_DISPLAY="$HOST_XDG/${WAYLAND_DISPLAY:-wayland-0}"
mkdir -p "$RUN_DIR" "$OUT"
chmod 700 "$RUNTIME_DIR"
NIRI_PID=""
ALACRITTY_PID=""

terminate_child() {
  local pid="$1"
  [ -n "$pid" ] || return 0
  kill "$pid" 2>/dev/null || true
  for _ in $(seq 1 20); do
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.1
  done
  if kill -0 "$pid" 2>/dev/null; then
    kill -KILL "$pid" 2>/dev/null || true
  fi
  wait "$pid" 2>/dev/null || true
}

cleanup() {
  terminate_child "$ALACRITTY_PID"
  terminate_child "$NIRI_PID"
  rm -rf "$RUNTIME_DIR"
  if [ -n "${TASKMGR_AGENT_LEASE:-}" ] && [ -d "$TASKMGR_AGENT_LEASE" ]; then
    rm -rf "$TASKMGR_AGENT_LEASE"
  fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

git -C "$REPO" status --short >"$RUN_DIR/git-status.txt"
git -C "$REPO" diff --binary >"$RUN_DIR/worktree.diff"
git -C "$REPO" diff --binary --cached >>"$RUN_DIR/worktree.diff"
PYTHONDONTWRITEBYTECODE=1 timeout 60s python3 scripts/frontend_source_manifest.py \
  --frontend tui --repo-root "$REPO" --output "$SOURCE_MANIFEST"
SOURCE_MANIFEST_SHA256="$(sha256sum "$SOURCE_MANIFEST" | cut -d' ' -f1)"

(cd "$REPO" && timeout --kill-after=10s 20m cargo build --locked --quiet --no-default-features --features ui-tui)

cat >"$CONF" <<KDL
screenshot-path "$RUN_DIR/shot-%Y-%m-%d-%H-%M-%S.png"
hotkey-overlay {
    skip-at-startup
}
window-rule {
    match app-id="taskmanager-tui"
    open-floating true
}
KDL
timeout 10s niri validate --config "$CONF"

# Software GL keeps the nested capture reliable: on hosts whose GPU context
# is degraded (KWin "atomic commit failed" storms), EGL initialization hangs
# and windows never map or present. llvmpipe renders the same frame; pixel
# content is unchanged. Applied to both the nested compositor and Alacritty.
XDG_RUNTIME_DIR="$RUNTIME_DIR" WAYLAND_DISPLAY="$HOST_DISPLAY" \
  LIBGL_ALWAYS_SOFTWARE=1 \
  niri --config "$CONF" >"$RUN_DIR/niri.log" 2>&1 &
NIRI_PID=$!

SOCK=""
IPC=""
for _ in $(seq 1 60); do
  SOCK="$(grep -oE 'wayland-[0-9]+' "$RUN_DIR/niri.log" | head -1 || true)"
  IPC="$(grep -oE "$RUNTIME_DIR/niri\.[^ ]*\.sock" "$RUN_DIR/niri.log" | head -1 || true)"
  if [ -n "$SOCK" ] && [ -S "$RUNTIME_DIR/$SOCK" ] && [ -n "$IPC" ]; then
    break
  fi
  sleep 0.2
done
if ! kill -0 "$NIRI_PID" 2>/dev/null || [ -z "$SOCK" ] || [ -z "$IPC" ]; then
  tail -20 "$RUN_DIR/niri.log"
  exit 1
fi

# Software GL keeps nested Alacritty's window creation reliable: on hosts
# whose GPU context is degraded (KWin "atomic commit failed" storms), EGL
# initialization against the nested compositor hangs and the terminal never
# maps a window. llvmpipe renders the same frame; pixel content is unchanged.
XDG_RUNTIME_DIR="$RUNTIME_DIR" WAYLAND_DISPLAY="$SOCK" \
  TM_TUI_CAPTURE_MARKER_FILE="$MARKERS" \
  TM_TUI_CAPTURE_PAGE="$CAPTURE_PAGE" \
  TM_TUI_CAPTURE_DEVICE="$CAPTURE_DEVICE" \
  TM_TUI_CAPTURE_SCENE="$CAPTURE_SCENE" \
  TM_TUI_CAPTURE_SOURCE_FAILURE="${TM_TUI_CAPTURE_SOURCE_FAILURE:-}" \
  LIBGL_ALWAYS_SOFTWARE=1 \
  alacritty --class taskmanager-tui --title "TaskForest TUI Evidence" \
    -o "window.dimensions.columns=$CAPTURE_COLUMNS" \
    -o "window.dimensions.lines=$CAPTURE_LINES" \
    -o 'window.padding.x=8' \
    -o 'window.padding.y=8' \
    -o "font.size=$CAPTURE_FONT_SIZE" \
    -e "$APP" --demo >"$RUN_DIR/alacritty.log" 2>&1 &
ALACRITTY_PID=$!

WINDOW_READY=0
for _ in $(seq 1 80); do
  if NIRI_SOCKET="$IPC" timeout 3s niri msg windows 2>/dev/null | grep -q 'taskmanager-tui'; then
    WINDOW_READY=1
    break
  fi
  sleep 0.25
done
for _ in $(seq 1 80); do
  if grep -q "TUI_CAPTURE_MARKER event=frame_ready page=$CAPTURE_PAGE" "$MARKERS" 2>/dev/null; then
    break
  fi
  sleep 0.25
done
if [ "$WINDOW_READY" -ne 1 ] || ! grep -q 'event=frame_ready' "$MARKERS" 2>/dev/null; then
  printf 'TUI window or frame marker did not become ready\n' >&2
  exit 1
fi
sleep 1
NIRI_SOCKET="$IPC" timeout 8 niri msg action screenshot-window >/dev/null
sleep 1
CAPTURED="$(ls -t "$RUN_DIR"/shot-*.png 2>/dev/null | head -1 || true)"
if [ -z "$CAPTURED" ] || [ "$(stat -c%s "$CAPTURED")" -lt 5000 ]; then
  printf 'TUI screenshot missing or too small\n' >&2
  exit 1
fi
mv "$CAPTURED" "$IMAGE"

RUST_VERSION="$(rustc -V)"
NIRI_VERSION="$(niri --version)"
CAPTURED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
{
  printf 'run_id=%s\n' "$RUN_ID"
  printf 'captured_at=%s\n' "$CAPTURED_AT"
  printf 'git_head=%s\n' "$GIT_HEAD"
  printf 'worktree=%s\n' "$WORKTREE_STATE"
  printf 'rust=%s\n' "$RUST_VERSION"
  printf 'niri=%s\n' "$NIRI_VERSION"
  printf 'terminal=%s\n' "$(alacritty --version)"
  printf 'stack=ratatui 0.30.2 + crossterm 0.29.0\n'
  printf 'source_scope=tui\n'
  printf 'page=%s\n' "$CAPTURE_PAGE"
  printf 'device=%s\n' "${CAPTURE_DEVICE:-default}"
  printf 'scene=%s\n' "${CAPTURE_SCENE:-default}"
  printf 'terminal_columns=%s\n' "$CAPTURE_COLUMNS"
  printf 'terminal_lines=%s\n' "$CAPTURE_LINES"
  printf 'terminal_font_size=%s\n' "$CAPTURE_FONT_SIZE"
  printf 'source_manifest_sha256=%s\n' "$SOURCE_MANIFEST_SHA256"
  printf 'command=bash scripts/capture-tui.sh\n'
} >"$METADATA"
printf 'image\twidth\theight\tbytes\tsha256\tmarkers\n' >"$MANIFEST"

PYTHONDONTWRITEBYTECODE=1 timeout 30s python3 "$REPO/scripts/validate_tui_evidence.py" \
  --image "$IMAGE" \
  --metadata "$METADATA" \
  --markers "$MARKERS" \
  --source-manifest "$SOURCE_MANIFEST" \
  --manifest "$MANIFEST" \
  --receipt "$RECEIPT" \
  --repo-root "$REPO" \
  --current-worktree

install -m0644 "$IMAGE" "$OUT/tui-mvp.png"
install -m0644 "$MARKERS" "$OUT/tui-capture-markers.log"
install -m0644 "$METADATA" "$OUT/tui-capture-metadata.txt"
install -m0644 "$MANIFEST" "$OUT/tui-capture-manifest.tsv"
install -m0644 "$RECEIPT" "$OUT/tui-capture-validation.json"
install -m0644 "$SOURCE_MANIFEST" "$OUT/tui-source-manifest.sha256"
printf '%s\n' "$RUN_ID" >"$EVIDENCE_ROOT/latest.txt"
printf 'TUI capture evidence: PASS -> %s\n' "$RUN_DIR"
