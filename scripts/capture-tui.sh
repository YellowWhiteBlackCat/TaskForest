#!/usr/bin/env bash
# Capture the real Ratatui/Crossterm frame inside Alacritty on nested Niri.
# The default acceptance path mirrors capture-niri.sh: nested Niri is hosted by
# a private virtual KWin framebuffer, so no capture window reaches the user's
# foreground desktop. Niri's screenshot-window action is bound to the exact
# Alacritty PID/app-id/window-id tuple rather than whichever window has focus.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
if [ "${TM_CAPTURE_SUPERVISED:-0}" != "1" ] \
  || [ "${TM_CAPTURE_SUPERVISOR_TOKEN:-}" != "${TM_CAPTURE_RUN_UUID:-}" ]; then
  command -v python3 >/dev/null 2>&1 \
    || { printf 'capture requires the supervisor interpreter\n' >&2; exit 2; }
  command -v timeout >/dev/null 2>&1 \
    || { printf 'capture requires timeout for supervisor lifetime bounding\n' >&2; exit 2; }
  exec timeout --kill-after=10s 30m python3 "$REPO/scripts/capture_supervisor.py" \
    --repo-root "$REPO" --frontend tui -- bash "$0" "$@"
fi
CAPTURE_RUN_UUID="${TM_CAPTURE_RUN_UUID:-}"
CAPTURE_RUN_ROOT="${TM_CAPTURE_RUN_ROOT:-}"
CAPTURE_RUNTIME_ROOT="${TM_CAPTURE_RUNTIME_ROOT:-}"
if [ -z "$CAPTURE_RUN_UUID" ] || [ -z "$CAPTURE_RUN_ROOT" ] || [ -z "$CAPTURE_RUNTIME_ROOT" ]; then
  printf 'capture must be started by the private supervisor\n' >&2
  exit 2
fi

if [ "${TM_CAPTURE_NIRI_BACKGROUND:-1}" = "1" ] \
  && [ "${TM_CAPTURE_PRIVATE_DBUS:-0}" != "1" ]; then
  command -v dbus-run-session >/dev/null 2>&1 \
    || { printf 'background TUI capture requires dbus-run-session\n' >&2; exit 2; }
  private_session_config="$(cd "$(dirname "$0")" && pwd)/private-session.conf"
  TM_CAPTURE_PRIVATE_DBUS=1 exec dbus-run-session \
    --config-file="$private_session_config" -- bash "$0" "$@"
fi

cd "$REPO"
eval "$(scripts/agent-workdir.sh enter tui-capture)"
# The shared sccache socket exceeds Linux SUN_LEN for this repository path;
# keep Cargo on the shared target but use rustc directly for this bounded run.
export RUSTC_WRAPPER=
# ADR-051: the TUI product is the taskmanager-tui crate's own bin.
APP="$CAPTURE_RUN_ROOT/bin/taskmanager-tui"
OUT="$REPO/target/tui-evidence/latest"
EVIDENCE_ROOT="$REPO/target/tui-evidence"
APP_ID="taskmanager-tui"
CAPTURE_PAGE="${TM_TUI_CAPTURE_PAGE:-performance}"
CAPTURE_DEVICE="${TM_TUI_CAPTURE_DEVICE:-}"
CAPTURE_SCENE="${TM_TUI_CAPTURE_SCENE:-}"
CAPTURE_COLUMNS="${TM_TUI_CAPTURE_COLUMNS:-120}"
CAPTURE_LINES="${TM_TUI_CAPTURE_LINES:-36}"
CAPTURE_FONT_SIZE="${TM_TUI_CAPTURE_FONT_SIZE:-13}"
CAPTURE_NIRI_BACKGROUND="${TM_CAPTURE_NIRI_BACKGROUND:-1}"
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
RUN_ID="$CAPTURE_RUN_UUID"
RUN_DIR="$CAPTURE_RUN_ROOT"
RUN_RELATIVE="${RUN_DIR#"$REPO/"}"
CONF="$RUN_DIR/config.kdl"
MARKERS="$RUN_DIR/tui-capture-markers.log"
METADATA="$RUN_DIR/tui-capture-metadata.txt"
MANIFEST="$RUN_DIR/tui-capture-manifest.tsv"
RECEIPT="$RUN_DIR/tui-capture-validation.json"
IMAGE="$RUN_DIR/tui-mvp.png"
SOURCE_MANIFEST="$RUN_DIR/tui-source-manifest.sha256"
HOST_XDG="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
HOST_DISPLAY="$HOST_XDG/${WAYLAND_DISPLAY:-wayland-0}"
NIRI_PARENT_WAYLAND="$HOST_DISPLAY"
NIRI_PID=""
NIRI_PGID=""
ALACRITTY_PID=""
ALACRITTY_PGID=""
WINDOW_PID=""
WINDOW_PGID=""
KWIN_PID=""
KWIN_PGID=""
KWIN_RUNTIME=""
KWIN_ROOT=""
KWIN_SOCKET=""
KWIN_DISPLAY=""

for command in cargo file git jq niri ps rustc sha256sum setsid stat timeout; do
  if ! command -v "$command" >/dev/null 2>&1; then
    printf 'required capture command is unavailable: %s\n' "$command" >&2
    exit 2
  fi
done
case "$CAPTURE_NIRI_BACKGROUND" in
  1) ;;
  0) printf 'visible TUI capture is disabled; use the private background route\n' >&2; exit 2 ;;
  *)
    printf 'TM_CAPTURE_NIRI_BACKGROUND must be 1: %s\n' \
      "$CAPTURE_NIRI_BACKGROUND" >&2
    exit 2
    ;;
esac
if [ "$CAPTURE_NIRI_BACKGROUND" -eq 1 ] \
  && ! command -v kwin_wayland >/dev/null 2>&1; then
  printf 'background TUI capture requires kwin_wayland --virtual; visible mode is disabled\n' >&2
  exit 2
fi
if [ "$CAPTURE_NIRI_BACKGROUND" -eq 1 ]; then
  case "${DBUS_SESSION_BUS_ADDRESS:-}" in
    unix:path=/tmp/dbus-*,guid=*) ;;
    *)
      printf 'background TUI capture requires the private-session D-Bus address\n' >&2
      exit 2
      ;;
  esac
fi
DBUS_ADDRESS_SHA256="$(printf '%s' "${DBUS_SESSION_BUS_ADDRESS:-}" | sha256sum | cut -d' ' -f1)"
case "$DBUS_ADDRESS_SHA256" in
  [0-9a-f][0-9a-f][0-9a-f][0-9a-f]*) ;;
  *) printf 'private D-Bus address hash could not be recorded\n' >&2; exit 2 ;;
esac

mkdir -p "$RUN_DIR" "$RUN_DIR/bin" "$EVIDENCE_ROOT"
RUNTIME_DIR="$CAPTURE_RUNTIME_ROOT/niri"
mkdir -p "$RUNTIME_DIR" "$RUNTIME_DIR/config" "$RUNTIME_DIR/data" \
  "$RUNTIME_DIR/cache" "$RUNTIME_DIR/state"
chmod 700 "$RUNTIME_DIR"

process_group() {
  local pid="$1"
  ps -o pgid= -p "$pid" 2>/dev/null | tr -d '[:space:]'
}

terminate_owned() {
  local pid="$1" pgid="$2"
  [ -n "$pid" ] || return 0
  if [[ "$pgid" =~ ^[0-9]+$ ]] && [ "$pgid" = "$pid" ]; then
    kill -TERM -- "-$pgid" 2>/dev/null || true
  else
    kill -TERM "$pid" 2>/dev/null || true
  fi
  for _ in $(seq 1 20); do
    if [[ "$pgid" =~ ^[0-9]+$ ]] && [ "$pgid" = "$pid" ]; then
      kill -0 -- "-$pgid" 2>/dev/null || break
    else
      kill -0 "$pid" 2>/dev/null || break
    fi
    sleep 0.1
  done
  if [[ "$pgid" =~ ^[0-9]+$ ]] && [ "$pgid" = "$pid" ]; then
    kill -0 -- "-$pgid" 2>/dev/null && kill -KILL -- "-$pgid" 2>/dev/null || true
  elif kill -0 "$pid" 2>/dev/null; then
    kill -KILL "$pid" 2>/dev/null || true
  fi
  wait "$pid" 2>/dev/null || true
}

cleanup() {
  terminate_owned "$WINDOW_PID" "$WINDOW_PGID"
  terminate_owned "$ALACRITTY_PID" "$ALACRITTY_PGID"
  terminate_owned "$NIRI_PID" "$NIRI_PGID"
  terminate_owned "$KWIN_PID" "$KWIN_PGID"
  if [ -n "$KWIN_ROOT" ] && [ -d "$KWIN_ROOT" ]; then
    case "$KWIN_ROOT" in
      "$CAPTURE_RUNTIME_ROOT/kwin") rm -rf -- "$KWIN_ROOT" ;;
      *) printf 'cleanup refused unexpected KWin root: %s\n' "$KWIN_ROOT" >&2 ;;
    esac
  fi
  if [ -d "$RUNTIME_DIR" ]; then
    rm -rf -- "$RUNTIME_DIR"
  fi
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

(cd "$REPO" && timeout --kill-after=10s 20m python3 scripts/capture_build.py \
  --repo-root "$REPO" --source "$REPO/target/debug/taskmanager-tui" \
  --destination "$APP" -- cargo build --locked --quiet -p taskmanager-tui --bin taskmanager-tui)
BINARY_SHA256="$(sha256sum "$APP" | cut -d' ' -f1)"
APP_RELATIVE="${APP#"$REPO/"}"

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

start_capture_host() {
  if [ "$CAPTURE_NIRI_BACKGROUND" -eq 0 ]; then
    return 0
  fi

  # Niri's nested winit backend is itself a host window. Put it inside a
  # private virtual KWin compositor so the capture never steals focus or
  # paints over the operator's desktop.
  KWIN_ROOT="$CAPTURE_RUNTIME_ROOT/kwin"
  KWIN_RUNTIME="$KWIN_ROOT/runtime"
  KWIN_SOCKET="wayland-${CAPTURE_RUN_UUID%%-*}"
  mkdir -p "$KWIN_ROOT/config" "$KWIN_ROOT/data" "$KWIN_ROOT/cache" \
    "$KWIN_ROOT/state" "$KWIN_RUNTIME"
  KWIN_DISPLAY="$KWIN_RUNTIME/$KWIN_SOCKET"
  chmod 700 "$KWIN_RUNTIME"
  XDG_RUNTIME_DIR="$KWIN_RUNTIME" XDG_CONFIG_HOME="$KWIN_ROOT/config" \
    XDG_DATA_HOME="$KWIN_ROOT/data" XDG_CACHE_HOME="$KWIN_ROOT/cache" \
    XDG_STATE_HOME="$KWIN_ROOT/state" WAYLAND_DISPLAY= DISPLAY= \
    LIBGL_ALWAYS_SOFTWARE=1 QT_QPA_PLATFORM=wayland \
    setsid timeout --foreground --kill-after=10s 20m \
    kwin_wayland --virtual --socket="$KWIN_SOCKET" --width=1920 --height=1080 \
      --scale=1 --no-global-shortcuts --no-lockscreen \
      >"$RUN_DIR/kwin-wayland.log" 2>&1 &
  KWIN_PID=$!
  KWIN_PGID=""
  for _ in $(seq 1 40); do
    KWIN_PGID="$(process_group "$KWIN_PID")"
    [ "$KWIN_PGID" = "$KWIN_PID" ] && break
    sleep 0.1
  done
  if [ "$KWIN_PGID" != "$KWIN_PID" ]; then
    printf 'virtual KWin did not obtain a private process group\n' >&2
    return 1
  fi
  for _ in $(seq 1 40); do
    [ -S "$KWIN_DISPLAY" ] && break
    sleep 0.2
  done
  if ! kill -0 "$KWIN_PID" 2>/dev/null || [ ! -S "$KWIN_DISPLAY" ]; then
    printf 'virtual KWin did not start; tail of log:\n' >&2
    tail -20 "$RUN_DIR/kwin-wayland.log" >&2 || true
    return 1
  fi
  NIRI_PARENT_WAYLAND="$KWIN_DISPLAY"
  printf 'background capture host: kwin-wayland --virtual (%s)\n' "$KWIN_DISPLAY"
}

start_capture_host || exit 1

# Software GL keeps the nested capture reliable: on hosts whose GPU context
# is degraded (KWin "atomic commit failed" storms), EGL initialization hangs
# and windows never map or present. llvmpipe renders the same frame; pixel
# content is unchanged. Applied to both the nested compositor and Alacritty.
XDG_RUNTIME_DIR="$RUNTIME_DIR" XDG_CONFIG_HOME="$RUNTIME_DIR/config" \
  XDG_DATA_HOME="$RUNTIME_DIR/data" XDG_CACHE_HOME="$RUNTIME_DIR/cache" \
  XDG_STATE_HOME="$RUNTIME_DIR/state" WAYLAND_DISPLAY="$NIRI_PARENT_WAYLAND" DISPLAY= \
  LIBGL_ALWAYS_SOFTWARE=1 RUST_LOG=niri=info \
  setsid timeout --foreground --kill-after=10s 20m niri --config "$CONF" \
  >"$RUN_DIR/niri.log" 2>&1 &
NIRI_PID=$!
NIRI_PGID="$(process_group "$NIRI_PID")"
[ "$NIRI_PGID" = "$NIRI_PID" ] || {
  printf 'nested Niri did not obtain a private process group\n' >&2
  exit 1
}

SOCK=""
IPC=""
for _ in $(seq 1 60); do
  SOCK="$(find "$RUNTIME_DIR" -maxdepth 1 -type s -name 'wayland-[0-9]*' \
    -printf '%f\n' -quit 2>/dev/null || true)"
  if [ -z "$SOCK" ]; then
    SOCK="$(grep -oE 'wayland-[0-9]+' "$RUN_DIR/niri.log" | head -1 || true)"
  fi
  IPC="$(find "$RUNTIME_DIR" -maxdepth 1 -type s -name 'niri.*.sock' \
    -print -quit 2>/dev/null || true)"
  if [ -z "$IPC" ]; then
    IPC="$(grep -oE "$RUNTIME_DIR/niri\.[^ ]*\.sock" "$RUN_DIR/niri.log" | head -1 || true)"
  fi
  if [ -n "$SOCK" ] && [ -S "$RUNTIME_DIR/$SOCK" ] && [ -n "$IPC" ]; then
    break
  fi
  sleep 0.2
done
if ! kill -0 "$NIRI_PID" 2>/dev/null || [ -z "$SOCK" ] || [ -z "$IPC" ]; then
  tail -20 "$RUN_DIR/niri.log" >&2
  exit 1
fi

# Software GL keeps nested Alacritty's window creation reliable: on hosts
# whose GPU context is degraded (KWin "atomic commit failed" storms), EGL
# initialization against the nested compositor hangs and the terminal never
# maps a window. llvmpipe renders the same frame; pixel content is unchanged.
XDG_RUNTIME_DIR="$RUNTIME_DIR" XDG_CONFIG_HOME="$RUNTIME_DIR/config" \
  XDG_DATA_HOME="$RUNTIME_DIR/data" XDG_CACHE_HOME="$RUNTIME_DIR/cache" \
  XDG_STATE_HOME="$RUNTIME_DIR/state" WAYLAND_DISPLAY="$SOCK" \
  TM_TUI_CAPTURE_MARKER_FILE="$MARKERS" \
  TM_TUI_CAPTURE_PAGE="$CAPTURE_PAGE" \
  TM_TUI_CAPTURE_DEVICE="$CAPTURE_DEVICE" \
  TM_TUI_CAPTURE_SCENE="$CAPTURE_SCENE" \
  TM_TUI_CAPTURE_SOURCE_FAILURE="${TM_TUI_CAPTURE_SOURCE_FAILURE:-}" \
  LIBGL_ALWAYS_SOFTWARE=1 \
  setsid alacritty --class "$APP_ID" --title "TaskForest TUI Evidence" \
    -o "window.dimensions.columns=$CAPTURE_COLUMNS" \
    -o "window.dimensions.lines=$CAPTURE_LINES" \
    -o 'window.padding.x=8' \
    -o 'window.padding.y=8' \
    -o "font.size=$CAPTURE_FONT_SIZE" \
    -e "$APP" --demo >"$RUN_DIR/alacritty.log" 2>&1 &
ALACRITTY_PID=$!
ALACRITTY_PGID="$(process_group "$ALACRITTY_PID")"

WINDOW_READY=0
WINDOWS="$RUN_DIR/windows.json"
WINDOWS_TMP="$RUN_DIR/windows.json.tmp"
WINDOW_ID=""
: >"$WINDOWS"
for _ in $(seq 1 80); do
  NIRI_SOCKET="$IPC" timeout 3s niri msg -j windows >"$WINDOWS_TMP" 2>/dev/null || true
  if [ -s "$WINDOWS_TMP" ]; then
    mv -f "$WINDOWS_TMP" "$WINDOWS"
  fi
  WINDOW_ID="$(jq -r --arg app "$APP_ID" \
    '[.[] | select(.app_id == $app)] | if length == 1 then .[0].id else empty end' \
    "$WINDOWS" 2>/dev/null || true)"
  WINDOW_PID="$(jq -r --arg app "$APP_ID" \
    '[.[] | select(.app_id == $app)] | if length == 1 then (.[0].pid|tostring) else empty end' \
    "$WINDOWS" 2>/dev/null || true)"
  if [ -n "$WINDOW_ID" ] && [ -n "$WINDOW_PID" ] \
    && kill -0 "$WINDOW_PID" 2>/dev/null; then
    WINDOW_PGID="$(process_group "$WINDOW_PID")"
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
ACTION="$RUN_DIR/action.log"
printf 'window_id=%s\n' "$WINDOW_ID" >"$ACTION"
printf 'action=screenshot-window --id %s --write-to-disk true --path %s\n' \
  "$WINDOW_ID" "$IMAGE" >>"$ACTION"
if ! NIRI_SOCKET="$IPC" timeout 8s niri msg action screenshot-window \
  --id "$WINDOW_ID" --write-to-disk true --path "$IMAGE" >>"$ACTION" 2>&1; then
  printf 'TUI screenshot-window action failed\n' >&2
  exit 1
fi
for _ in $(seq 1 50); do
  if [ -s "$IMAGE" ] && file "$IMAGE" | grep -q 'PNG image data'; then
    break
  fi
  sleep 0.1
done
if [ ! -s "$IMAGE" ] || ! file "$IMAGE" | grep -q 'PNG image data' \
  || [ "$(stat -c%s "$IMAGE")" -lt 5000 ]; then
  printf 'TUI screenshot missing or too small\n' >&2
  exit 1
fi

RUST_VERSION="$(rustc -V)"
NIRI_VERSION="$(niri --version)"
CAPTURED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
{
  printf 'run_id=%s\n' "$RUN_ID"
  printf 'run_uuid=%s\n' "$CAPTURE_RUN_UUID"
  printf 'frontend=tui\n'
  printf 'run_root=%s\n' "$RUN_RELATIVE"
  printf 'runtime_root=%s\n' "$CAPTURE_RUNTIME_ROOT"
  printf 'supervisor_pid=%s\n' "${TM_CAPTURE_SUPERVISOR_PID:-}"
  printf 'cgroup_path=%s\n' "${TM_CAPTURE_CGROUP_PATH:-}"
  printf 'captured_at=%s\n' "$CAPTURED_AT"
  printf 'git_head=%s\n' "$GIT_HEAD"
  printf 'worktree=%s\n' "$WORKTREE_STATE"
  printf 'rust=%s\n' "$RUST_VERSION"
  printf 'niri=%s\n' "$NIRI_VERSION"
  printf 'terminal=%s\n' "$(alacritty --version)"
  printf 'stack=ratatui 0.30.2 + crossterm 0.29.0\n'
  printf 'app_id=%s\n' "$APP_ID"
  printf 'binary=%s\n' "$APP_RELATIVE"
  printf 'binary_sha256=%s\n' "$BINARY_SHA256"
  printf 'app_pid=%s\n' "$WINDOW_PID"
  printf 'launcher_pid=%s\n' "$ALACRITTY_PID"
  printf 'window_id=%s\n' "$WINDOW_ID"
  printf 'capture_backend=niri-screenshot-window-wayland\n'
  if [ "$CAPTURE_NIRI_BACKGROUND" -eq 1 ]; then
    printf 'niri_host=kwin-wayland-virtual\n'
  else
    printf 'niri_host=host-wayland-visible\n'
  fi
  printf 'niri_background=%s\n' "$CAPTURE_NIRI_BACKGROUND"
  printf 'dbus_isolation=private-session\n'
  printf 'dbus_address_sha256=%s\n' "$DBUS_ADDRESS_SHA256"
  printf 'windows_receipt=%s\n' "$WINDOWS"
  printf 'action_receipt=%s\n' "$ACTION"
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

PYTHONDONTWRITEBYTECODE=1 timeout --kill-after=5s 30s python3 "$REPO/scripts/capture_publish.py" \
  --repo-root "$REPO" --frontend tui --run-root "$RUN_DIR" --run-uuid "$RUN_ID"
printf 'TUI capture evidence: PASS -> %s\n' "$RUN_DIR"
