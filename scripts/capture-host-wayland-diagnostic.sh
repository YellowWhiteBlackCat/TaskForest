#!/usr/bin/env bash
# Run one current-build GPUI capture against a private Wayland compositor.
#
# This is a diagnostic path, never a parity publisher. It deliberately uses
# KWin/Spectacle's active-window capture so native-provider and "still
# collecting" failures can be observed without touching the host compositor.
# Every artifact stays below target/host-wayland-diagnostic/.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
if [ "${TM_CAPTURE_SUPERVISED:-0}" != "1" ] \
  || [ "${TM_CAPTURE_SUPERVISOR_TOKEN:-}" != "${TM_CAPTURE_RUN_UUID:-}" ]; then
  command -v python3 >/dev/null 2>&1 \
    || { printf 'capture requires the supervisor interpreter\n' >&2; exit 2; }
  command -v timeout >/dev/null 2>&1 \
    || { printf 'capture requires timeout for supervisor lifetime bounding\n' >&2; exit 2; }
  exec timeout --kill-after=10s 30m python3 "$REPO/scripts/capture_supervisor.py" \
    --repo-root "$REPO" --frontend host-wayland -- bash "$0" "$@"
fi
CAPTURE_RUN_UUID="${TM_CAPTURE_RUN_UUID:-}"
CAPTURE_RUN_ROOT="${TM_CAPTURE_RUN_ROOT:-}"
CAPTURE_RUNTIME_ROOT="${TM_CAPTURE_RUNTIME_ROOT:-}"
if [ -z "$CAPTURE_RUN_UUID" ] || [ -z "$CAPTURE_RUN_ROOT" ] || [ -z "$CAPTURE_RUNTIME_ROOT" ]; then
  printf 'capture must be started by the private supervisor\n' >&2
  exit 2
fi
EVIDENCE_ROOT="$REPO/target/host-wayland-diagnostic"
APP="$CAPTURE_RUN_ROOT/bin/taskforest-g"
PRIVATE_KWIN="${TM_HOST_WAYLAND_PRIVATE_KWIN:-1}"
SCENARIO="${TM_HOST_WAYLAND_SCENARIO:-standard}"
SKIN="${TM_HOST_WAYLAND_SKIN:-gnome-light}"
PAGE="${TM_HOST_WAYLAND_PAGE:-performance}"
DEVICE="${TM_HOST_WAYLAND_DEVICE:-cpu}"
SETTINGS="${TM_HOST_WAYLAND_SETTINGS:-0}"
WINDOW_SIZE="${TM_HOST_WAYLAND_WINDOW_SIZE:-1180x780}"

if [ "$PRIVATE_KWIN" = "1" ] && [ "${TM_PRIVATE_KWIN_BUS:-0}" != "1" ]; then
  TM_PRIVATE_KWIN_BUS=1 exec dbus-run-session \
    --config-file="$REPO/scripts/private-session.conf" -- bash "$0" "$@"
fi
if [ "$PRIVATE_KWIN" != "1" ]; then
  printf 'host diagnostic refuses non-private compositor mode; set TM_HOST_WAYLAND_PRIVATE_KWIN=1\n' >&2
  exit 2
fi
case "${DBUS_SESSION_BUS_ADDRESS:-}" in
  unix:path=/tmp/dbus-*,guid=*) ;;
  *)
    printf 'private KWin diagnostic requires the private-session D-Bus address\n' >&2
    exit 2
    ;;
esac
DBUS_ADDRESS_SHA256="$(printf '%s' "$DBUS_SESSION_BUS_ADDRESS" | sha256sum | cut -d' ' -f1)"

usage() {
  cat <<'USAGE'
Usage: bash scripts/capture-host-wayland-diagnostic.sh [SCENARIO]

Runs one non-publishing active-window diagnostic on a private virtual KWin
Wayland session. The image and receipts are written only below
target/host-wayland-diagnostic/. A scale other than 1, missing strict markers,
an OCR-detected telemetry skeleton, a stale build, or any receipt mismatch is
an error. No result from this script is durable parity evidence.

Environment overrides: TM_HOST_WAYLAND_{SKIN,PAGE,DEVICE,SETTINGS,WINDOW_SIZE}.
The optional positional SCENARIO overrides TM_HOST_WAYLAND_SCENARIO.
The private virtual KWin route is mandatory; visible host capture is rejected.
USAGE
}

if [ "${1:-}" = "--help" ] || [ "${1:-}" = "-h" ]; then
  usage
  exit 0
fi
if [ "$#" -gt 1 ]; then
  usage >&2
  exit 2
fi
[ -z "${1:-}" ] || SCENARIO="$1"

case "$SCENARIO" in
  standard|[a-z0-9-]*) ;;
  *) printf 'invalid capture scenario: %s\n' "$SCENARIO" >&2; exit 2 ;;
esac
case "$SKIN" in
  gnome-light|gnome-dark|kde-light|kde-dark|windows-light|windows-dark|macos-light|macos-dark) ;;
  *) printf 'invalid capture theme: %s\n' "$SKIN" >&2; exit 2 ;;
esac
case "$WINDOW_SIZE" in
  [1-9]*x[1-9]*) ;;
  *) printf 'invalid capture window size: %s\n' "$WINDOW_SIZE" >&2; exit 2 ;;
esac

HOST_XDG="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
HOST_DISPLAY="${WAYLAND_DISPLAY:-wayland-0}"
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
LOG="$RUN_DIR/app.log"
MARKERS="$RUN_DIR/markers.log"
METADATA="$RUN_DIR/metadata.txt"
IMAGE="$RUN_DIR/host.png"
NATIVE_IMAGE="$RUN_DIR/native-window.png"
OCR="$RUN_DIR/ocr.txt"
RECEIPT="$RUN_DIR/validation.json"
SOURCE_MANIFEST="$RUN_DIR/gpui-source-manifest.sha256"
WINDOW_INFO="$RUN_DIR/window-info.txt"
KWIN_LOG="$RUN_DIR/kwin-wayland.log"
KWIN_SCRIPT="$RUN_DIR/window-info.js"
RECEIVER_PID=""
RECEIVER_READY="$RUN_DIR/window-info.receiver-ready"
DISPLAY_RAW="$RUN_DIR/display-raw.txt"
DISPLAY_NORMALIZED="$RUN_DIR/display-normalized.txt"
SCRATCH=""
APP_PID=""
AGENT_LEASE=""
KWIN_PID=""
KWIN_START_TIME=""
KWIN_RUNTIME=""
KWIN_ROOT=""
KWIN_SOCKET=""
KWIN_DISPLAY=""

mkdir -p "$RUN_DIR" "$RUN_DIR/bin"
chmod 700 "$RUN_DIR"

terminate_child() {
  local pid="$1"
  [ -n "$pid" ] || return 0
  if ! kill -0 "$pid" 2>/dev/null; then
    wait "$pid" 2>/dev/null || true
    return 0
  fi

  # Refuse to signal a reused PID. The only process this script may terminate
  # is the executable it launched and recorded below.
  local current_exe=""
  current_exe="$(readlink -f "/proc/$pid/exe" 2>/dev/null || true)"
  if [ -z "$current_exe" ] || [ "$current_exe" != "$APP" ]; then
    printf 'cleanup refused non-owned PID=%s exe=%s\n' "$pid" "$current_exe" >&2
    return 1
  fi
  kill -TERM "$pid" 2>/dev/null || true
  for _ in $(seq 1 30); do
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.1
  done
  if kill -0 "$pid" 2>/dev/null; then
    current_exe="$(readlink -f "/proc/$pid/exe" 2>/dev/null || true)"
    if [ "$current_exe" = "$APP" ]; then
      kill -KILL "$pid" 2>/dev/null || true
    else
      printf 'cleanup refused reused PID=%s exe=%s\n' "$pid" "$current_exe" >&2
      return 1
    fi
  fi
  wait "$pid" 2>/dev/null || true
}

record_processes() {
  local label="$1"
  {
    printf 'label=%s\n' "$label"
    printf 'app_pid=%s\n' "$APP_PID"
    if [ -n "$APP_PID" ]; then
      ps -o pid=,ppid=,pgid=,etimes=,%cpu=,stat=,args= -p "$APP_PID" 2>/dev/null || true
      if [ -e "/proc/$APP_PID/exe" ]; then
        printf 'app_exe=%s\n' "$(readlink -f "/proc/$APP_PID/exe" 2>/dev/null || printf 'unavailable')"
      fi
    fi
  } >"$RUN_DIR/process-$label.txt"
}

cleanup() {
  local cleanup_status=0
  set +e
  record_processes before-cleanup
  if [ -n "$APP_PID" ]; then
    terminate_child "$APP_PID" || cleanup_status=1
  fi
  if [ -n "$KWIN_PID" ] && kill -0 "$KWIN_PID" 2>/dev/null; then
    kwin_exe="$(readlink -f "/proc/$KWIN_PID/exe" 2>/dev/null || true)"
    kwin_args="$(ps -o args= -p "$KWIN_PID" 2>/dev/null || true)"
    if [[ "$kwin_args" == *"kwin_wayland --virtual --socket=$KWIN_SOCKET"* ]]; then
      kwin_pgid="$(ps -o pgid= -p "$KWIN_PID" 2>/dev/null | tr -d '[:space:]')"
      if [ "$kwin_pgid" = "$KWIN_PID" ]; then
        kill -TERM -- "-$kwin_pgid" 2>/dev/null || true
      else
        kill -TERM "$KWIN_PID" 2>/dev/null || true
      fi
      for _ in $(seq 1 30); do
        kill -0 "$KWIN_PID" 2>/dev/null || break
        sleep 0.1
      done
      if kill -0 "$KWIN_PID" 2>/dev/null; then
        if [ "$kwin_pgid" = "$KWIN_PID" ]; then
          kill -KILL -- "-$kwin_pgid" 2>/dev/null || true
        else
          kill -KILL "$KWIN_PID" 2>/dev/null || true
        fi
      fi
      wait "$KWIN_PID" 2>/dev/null || true
    else
      printf 'cleanup refused non-owned KWin PID=%s exe=%s args=%s\n' \
        "$KWIN_PID" "$kwin_exe" "$kwin_args" >&2
      cleanup_status=1
    fi
  fi
  if [ -n "$RECEIVER_PID" ] && kill -0 "$RECEIVER_PID" 2>/dev/null; then
    receiver_args="$(ps -o args= -p "$RECEIVER_PID" 2>/dev/null || true)"
    if [[ "$receiver_args" == *"receive_kwin_window_receipt.py"* ]]; then
      kill -TERM "$RECEIVER_PID" 2>/dev/null || true
      wait "$RECEIVER_PID" 2>/dev/null || true
    else
      printf 'cleanup refused non-owned KWin receipt receiver PID=%s args=%s\n' \
        "$RECEIVER_PID" "$receiver_args" >&2
      cleanup_status=1
    fi
  fi
  record_processes after-cleanup
  if [ -n "$APP_PID" ] && kill -0 "$APP_PID" 2>/dev/null; then
    cleanup_status=1
    printf 'residue=app_pid_%s\n' "$APP_PID" >"$RUN_DIR/cleanup-failure.txt"
  else
    printf 'residue=none\n' >"$RUN_DIR/cleanup-status.txt"
  fi
  if [ -n "$SCRATCH" ] && [ -d "$SCRATCH" ]; then
    rm -rf -- "$SCRATCH"
  fi
  if [ -n "$AGENT_LEASE" ] && [ -d "$AGENT_LEASE" ]; then
    case "$AGENT_LEASE" in
      "$REPO/.tmp/agent-runs/"*) rm -rf -- "$AGENT_LEASE" ;;
      *) printf 'cleanup refused unexpected lease path: %s\n' "$AGENT_LEASE" >&2; cleanup_status=1 ;;
    esac
  fi
  if [ "$cleanup_status" -ne 0 ]; then
    printf 'cleanup=failed\n' >>"$RUN_DIR/cleanup-failure.txt"
  fi
  if [ -n "$KWIN_ROOT" ] && [ -d "$KWIN_ROOT" ]; then
    case "$KWIN_ROOT" in
      "$CAPTURE_RUNTIME_ROOT/kwin") rm -rf -- "$KWIN_ROOT" ;;
      *) printf 'cleanup refused unexpected KWin root: %s\n' "$KWIN_ROOT" >&2 ;;
    esac
  fi
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

fail() {
  printf '%s\n' "$*" >"$RUN_DIR/runner-failure.txt"
  printf 'HOST WAYLAND DIAGNOSTIC REJECTED: %s\n' "$*" >&2
  exit 1
}

# Cargo must use the repository NVMe target and a lease-owned scratch directory.
# Re-install our trap after enter: the lease manager's generated EXIT trap is
# intentionally superseded by cleanup(), which removes the exact recorded lease
# after the app PID has been audited.
eval "$("$REPO/scripts/agent-workdir.sh" enter host-wayland-diagnostic)"
AGENT_LEASE="${TASKMGR_AGENT_LEASE:-}"
case "$AGENT_LEASE" in
  "$REPO/.tmp/agent-runs/"*) ;;
  *) fail "agent-workdir did not return an owned repository lease" ;;
esac
trap cleanup EXIT

SCRATCH="$(mktemp -d "$TMPDIR/taskmanager-host-wayland.XXXXXX")"
EXPECTED_TARGET="$(readlink -f "$REPO/target")"
ACTUAL_TARGET="$(readlink -f "$CARGO_TARGET_DIR")"
[ "$ACTUAL_TARGET" = "$EXPECTED_TARGET" ] || fail "CARGO_TARGET_DIR is not the shared repository target"
case "$(readlink -f "$TMPDIR")" in
  "$REPO/.tmp/agent-runs/"*) ;;
  *) fail "TMPDIR is outside the repository agent-run lease" ;;
esac

if [ "$PRIVATE_KWIN" != "1" ] && [ "${XDG_SESSION_TYPE:-}" != "wayland" ]; then
  fail "host diagnostic requires XDG_SESSION_TYPE=wayland"
fi
for command_name in cargo timeout spectacle file sha256sum; do
  command -v "$command_name" >/dev/null 2>&1 || fail "required command is unavailable: $command_name"
done
if [ "$PRIVATE_KWIN" != "1" ]; then
  command -v kscreen-doctor >/dev/null 2>&1 || fail "required command is unavailable: kscreen-doctor"
fi
if [ "$PRIVATE_KWIN" = "1" ]; then
  for command_name in kwin_wayland qdbus6 python3; do
    command -v "$command_name" >/dev/null 2>&1 || fail "private KWin capture needs $command_name"
  done
  KWIN_ROOT="$CAPTURE_RUNTIME_ROOT/kwin"
  KWIN_RUNTIME="$KWIN_ROOT/runtime"
  mkdir -p "$KWIN_ROOT/config" "$KWIN_ROOT/data" "$KWIN_ROOT/cache" \
    "$KWIN_ROOT/state" "$KWIN_RUNTIME"
  chmod 700 "$KWIN_RUNTIME"
  KWIN_SOCKET="wayland-${CAPTURE_RUN_UUID%%-*}"
  KWIN_DISPLAY="$KWIN_RUNTIME/$KWIN_SOCKET"
  XDG_RUNTIME_DIR="$KWIN_RUNTIME" XDG_CONFIG_HOME="$KWIN_ROOT/config" \
    XDG_DATA_HOME="$KWIN_ROOT/data" XDG_CACHE_HOME="$KWIN_ROOT/cache" \
    XDG_STATE_HOME="$KWIN_ROOT/state" WAYLAND_DISPLAY= DISPLAY= QT_QPA_PLATFORM=wayland \
    QT_IM_MODULE=none GTK_IM_MODULE=none XMODIFIERS=@im=none \
    LIBGL_ALWAYS_SOFTWARE=1 \
    setsid timeout --foreground --kill-after=10s 20m \
    kwin_wayland --virtual --socket="$KWIN_SOCKET" --width=1180 --height=780 \
      --scale=1 --no-global-shortcuts --no-lockscreen \
      &>"$KWIN_LOG" & KWIN_PID=$!
  HOST_XDG="$KWIN_RUNTIME"
  HOST_DISPLAY="$KWIN_SOCKET"
  for _ in $(seq 1 60); do
    [ -S "$KWIN_DISPLAY" ] && break
    sleep 0.2
  done
  [ -S "$KWIN_DISPLAY" ] || fail "private virtual KWin Wayland socket did not appear"
  for _ in $(seq 1 60); do
    if qdbus6 org.kde.KWin /KWin >/dev/null 2>&1; then
      break
    fi
    sleep 0.2
  done
  qdbus6 org.kde.KWin /KWin >/dev/null 2>&1 || fail "private KWin D-Bus service did not appear"
fi
[ -S "$HOST_XDG/$HOST_DISPLAY" ] || fail "Wayland socket is unavailable: $HOST_XDG/$HOST_DISPLAY"
if [ "$PRIVATE_KWIN" = "1" ]; then
  KWIN_START_TIME="$(awk '{print $22}' "/proc/$KWIN_PID/stat" 2>/dev/null || true)"
  [ -n "$KWIN_START_TIME" ] || fail "private KWin PID has no start-time receipt"
fi

RUST_VERSION="$(rustc -V 2>/dev/null || printf 'unavailable')"
CAPTURED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# The repository's normal Cargo config may enable sccache. Its Unix socket can
# exceed SUN_LEN when this runner is launched from a long agent lease path, so
# the diagnostic build deliberately disables the wrapper while retaining the
# exact locked current-worktree command recorded in the receipt.
if ! (cd "$REPO" && RUSTC_WRAPPER= timeout --kill-after=10s 20m python3 scripts/capture_build.py \
  --repo-root "$REPO" --source "$REPO/target/debug/taskforest-g" \
  --destination "$APP" -- cargo build --locked --quiet \
  -p taskmanager-gpui --bin taskforest-g); then
  fail "locked current-worktree build failed"
fi
[ -x "$APP" ] || fail "current build did not produce $APP"
BINARY_SHA256="$(sha256sum "$APP" | cut -d' ' -f1)"
APP_RELATIVE="${APP#"$REPO/"}"
PYTHONDONTWRITEBYTECODE=1 timeout 60s python3 "$REPO/scripts/frontend_source_manifest.py" \
  --frontend gpui --repo-root "$REPO" --output "$SOURCE_MANIFEST" \
  || fail "current GPUI source manifest could not be generated"
SOURCE_MANIFEST_SHA256="$(sha256sum "$SOURCE_MANIFEST" | cut -d' ' -f1)"

if [ "$PRIVATE_KWIN" = "1" ]; then
  # kscreen-doctor talks to the host display service, not the isolated virtual
  # KWin instance. The private compositor command and accepted PNG are the
  # authoritative scale/geometry receipt for this non-host run.
  DISPLAY_SCALE=1
  DISPLAY_GEOMETRY="0,0 1180x780"
  DISPLAY_MODE="private-virtual-kwin 1180x780@1"
  printf 'private_virtual_kwin=true\nscale=1\ngeometry=%s\nmode=%s\n' \
    "$DISPLAY_GEOMETRY" "$DISPLAY_MODE" >"$DISPLAY_RAW"
  cp "$DISPLAY_RAW" "$DISPLAY_NORMALIZED"
else
  if ! timeout --kill-after=3s 10s kscreen-doctor -o >"$DISPLAY_RAW" 2>&1; then
    fail "kscreen-doctor could not provide a display scale receipt"
  fi
  sed -E 's/\x1B\[[0-9;]*m//g' "$DISPLAY_RAW" >"$DISPLAY_NORMALIZED"
  DISPLAY_SCALE="$(sed -nE 's/^[[:space:]]*Scale:[[:space:]]*([0-9]+([.][0-9]+)?).*/\1/p' "$DISPLAY_NORMALIZED" | head -1)"
  DISPLAY_GEOMETRY="$(sed -nE 's/^[[:space:]]*Geometry:[[:space:]]*(.*)/\1/p' "$DISPLAY_NORMALIZED" | head -1 | tr '\t' ' ')"
  DISPLAY_MODE="$(sed -nE 's/^[[:space:]]*Modes:[[:space:]]*(.*)/\1/p' "$DISPLAY_NORMALIZED" | head -1 | tr '\t' ' ')"
  [ -n "$DISPLAY_SCALE" ] || fail "display scale was absent from kscreen-doctor receipt"
  [ -n "$DISPLAY_GEOMETRY" ] || fail "display geometry was absent from kscreen-doctor receipt"
fi

mkdir -p "$RUN_DIR/config" "$RUN_DIR/state" "$RUN_DIR/data" "$RUN_DIR/cache"
TM_CAPTURE_SCENARIO="$SCENARIO"
[ "$SCENARIO" = standard ] && TM_CAPTURE_SCENARIO=""
launch_env=(
  "XDG_RUNTIME_DIR=$HOST_XDG"
  "XDG_CONFIG_HOME=$RUN_DIR/config"
  "XDG_STATE_HOME=$RUN_DIR/state"
  "XDG_DATA_HOME=$RUN_DIR/data"
  "XDG_CACHE_HOME=$RUN_DIR/cache"
  "WAYLAND_DISPLAY=$HOST_DISPLAY"
  "WINIT_UNIX_BACKEND=wayland"
  "LANG=C.UTF-8"
  "LANGUAGE=en_US:en"
  "LC_ALL=C.UTF-8"
  "TM_SKIN=$SKIN"
  "TM_PAGE=$PAGE"
  "TM_DEVICE=$DEVICE"
  "TM_SETTINGS=$SETTINGS"
  "TM_SKIN_HC="
  "TM_CAPTURE_EVIDENCE=1"
  "TM_CAPTURE_SCENARIO=$TM_CAPTURE_SCENARIO"
  "TM_WINDOW_SIZE=$WINDOW_SIZE"
  "QT_IM_MODULE=none"
  "GTK_IM_MODULE=none"
  "XMODIFIERS=@im=none"
)
if [ "$PRIVATE_KWIN" = "1" ]; then
  launch_env+=("TM_CAPTURE_WINDOW_CHAIN=1" "TM_CAPTURE_WINDOW_OUTPUT=$NATIVE_IMAGE")
fi
env "${launch_env[@]}" "$APP" &>"$LOG" &
APP_PID=$!

if ! kill -0 "$APP_PID" 2>/dev/null || [ ! -r "/proc/$APP_PID/stat" ]; then
  fail "current-build app exited before the host diagnostic began"
fi
APP_START_TIME="$(awk '{print $22}' "/proc/$APP_PID/stat")"
APP_EXE="$(readlink -f "/proc/$APP_PID/exe" 2>/dev/null || true)"
[ "$APP_EXE" = "$APP" ] || fail "launched PID does not resolve to the current binary"
[ -n "$APP_START_TIME" ] || fail "launched PID has no start-time receipt"

marker_scenario="$SCENARIO"
theme_marker="CAPTURE_MARKER event=theme_ready scenario=$marker_scenario theme=$SKIN high_contrast=false"
ready=0
for _ in $(seq 1 100); do
  if ! kill -0 "$APP_PID" 2>/dev/null; then
    fail "current-build app exited before strict markers became ready"
  fi
  if grep -Fq "$theme_marker" "$LOG" 2>/dev/null \
    && grep -Fq "CAPTURE_MARKER event=telemetry_ready scenario=$marker_scenario" "$LOG" 2>/dev/null \
    && grep -Fq "CAPTURE_MARKER event=ui_data_ready scenario=$marker_scenario" "$LOG" 2>/dev/null \
    && { [ "$SCENARIO" = standard ] || grep -Fq "CAPTURE_MARKER event=scenario_ready scenario=$SCENARIO" "$LOG" 2>/dev/null; }; then
    ready=1
    break
  fi
  sleep 0.25
done
[ "$ready" -eq 1 ] || fail "strict marker deadline expired; no image was accepted"

if [ "$PRIVATE_KWIN" = "1" ]; then
  native_ready=0
  for _ in $(seq 1 120); do
    if ! kill -0 "$APP_PID" 2>/dev/null; then
      fail "current-build app exited before native active-window completion"
    fi
    if grep -Fq 'current-window PNG capture completed' "$LOG" 2>/dev/null; then
      native_ready=1
      break
    fi
    sleep 0.25
  done
  [ "$native_ready" -eq 1 ] || fail "native active-window completion marker deadline expired"
fi

# Give the marker-triggered frame a bounded settle interval. The validator's
# OCR and pixel checks still reject a stable "Collecting telemetry…" skeleton.
sleep 2
if ! kill -0 "$APP_PID" 2>/dev/null; then
  fail "current-build app exited before active-window capture"
fi
if [ "$(readlink -f "/proc/$APP_PID/exe" 2>/dev/null || true)" != "$APP" ]; then
  fail "app PID ownership changed before capture"
fi

if [ "$PRIVATE_KWIN" = "1" ]; then
  # queryWindowInfo() is intentionally interactive in KWin and returns
  # UserCancel without a pointer selection. Use the supported scripting API's
  # stackingOrder/activeWindow model instead, and publish one bounded typed
  # line to the private D-Bus receiver.
  printf '%s\n' \
    "var targetPid = $APP_PID;" \
    'var attempts = 0;' \
    'function inspectWindows() {' \
    '    var windows = workspace.stackingOrder;' \
    '    for (var i = 0; i < windows.length; i++) {' \
    '        var window = windows[i];' \
    '        if (String(window.pid) !== String(targetPid)) continue;' \
    '        var payload = "TASKFOREST_WINDOW pid=" + window.pid + " active=" + window.active +' \
    '            " caption=" + window.caption + " resourceClass=" + window.resourceClass +' \
    '            " desktopFileName=" + window.desktopFileName +' \
    '            " width=" + window.width + " height=" + window.height +' \
    '            " x=" + window.x + " y=" + window.y + " internalId=" + window.internalId;' \
    '        callDBus("io.github.YellowWhiteBlackCat.TaskForest.CaptureReceipt", "/Capture", "io.github.YellowWhiteBlackCat.CaptureReceipt", "publish", payload);' \
    '        return;' \
    '    }' \
    '    attempts += 1;' \
    '    if (attempts < 80) {' \
    '        setTimeout(inspectWindows, 100);' \
    '    } else {' \
    '        callDBus("io.github.YellowWhiteBlackCat.TaskForest.CaptureReceipt", "/Capture", "io.github.YellowWhiteBlackCat.CaptureReceipt", "publish", "TASKFOREST_WINDOW_NOT_FOUND pid=" + targetPid);' \
    '    }' \
    '}' \
    'inspectWindows();' >"$KWIN_SCRIPT"
  rm -f "$WINDOW_INFO.tmp" "$RECEIVER_READY"
  PYTHONDONTWRITEBYTECODE=1 timeout --kill-after=3s 10s python3 "$REPO/scripts/receive_kwin_window_receipt.py" \
    "$WINDOW_INFO.tmp" "$RECEIVER_READY" >"$WINDOW_INFO.receiver.out" \
    2>"$WINDOW_INFO.receiver.err" &
  RECEIVER_PID=$!
  for _ in $(seq 1 40); do
    [ -f "$RECEIVER_READY" ] && break
    sleep 0.1
  done
  [ -f "$RECEIVER_READY" ] || fail "private KWin receipt receiver did not start"
  script_id="$(qdbus6 org.kde.KWin /Scripting org.kde.kwin.Scripting.loadScript \
    "$KWIN_SCRIPT" "taskforest-rc7-window-receipt" 2>"$WINDOW_INFO.load.err" || true)"
  script_id="$(printf '%s\n' "$script_id" | tr -d '[:space:]')"
  printf '%s\n' "$script_id" >"$WINDOW_INFO.script-id"
  if [ -n "$script_id" ]; then
    qdbus6 org.kde.KWin "/Scripting/Script${script_id}" >"$WINDOW_INFO.script-introspection" \
      2>"$WINDOW_INFO.script-introspection.err" || true
    qdbus6 org.kde.KWin "/Scripting/Script${script_id}" \
      org.kde.kwin.Script.run >"$WINDOW_INFO.run.out" 2>"$WINDOW_INFO.run.err" || true
    sleep 0.5
    qdbus6 org.kde.KWin "/Scripting/Script${script_id}" \
      org.kde.kwin.Script.stop >"$WINDOW_INFO.stop.out" 2>"$WINDOW_INFO.stop.err" || true
  fi
  if ! wait "$RECEIVER_PID" 2>/dev/null; then
    :
  fi
  RECEIVER_PID=""
  grep -F 'TASKFOREST_WINDOW ' "$WINDOW_INFO.tmp" >"$WINDOW_INFO" 2>/dev/null || :
  [ -s "$WINDOW_INFO" ] || fail "private KWin active-window identity did not bind to TaskForestG"
  grep -Fq "TASKFOREST_WINDOW pid=$APP_PID " "$WINDOW_INFO" \
    || fail "private KWin window receipt did not contain the launched app PID"
  grep -Fq 'active=true' "$WINDOW_INFO" \
    || fail "private KWin window receipt did not prove the app was active"
  WINDOW_INFO_SHA256="$(sha256sum "$WINDOW_INFO" | cut -d' ' -f1)"
fi

rm -f "$IMAGE"
if ! timeout --kill-after=5s 30s spectacle --activewindow --background --nonotify \
  --no-decoration --no-shadow --output "$IMAGE"; then
  fail "bounded Spectacle active-window capture failed"
fi
[ -s "$IMAGE" ] || fail "active-window capture produced no PNG"
[ "$(stat -c%s "$IMAGE")" -gt 5000 ] || fail "active-window PNG is too small"

grep -F 'CAPTURE_MARKER' "$LOG" >"$MARKERS" || true
IMAGE_DESCRIPTION="$(timeout --kill-after=2s 5s file "$IMAGE" || true)"
IMAGE_DIMS="$(printf '%s\n' "$IMAGE_DESCRIPTION" | sed -nE 's/.*PNG image data, ([0-9]+) x ([0-9]+).*/\1 \2/p')"
read -r IMAGE_WIDTH IMAGE_HEIGHT <<<"${IMAGE_DIMS:-0 0}"
IMAGE_WIDTH="${IMAGE_WIDTH:-0}"
IMAGE_HEIGHT="${IMAGE_HEIGHT:-0}"
IMAGE_BYTES="$(stat -c%s "$IMAGE")"
IMAGE_SHA256="$(sha256sum "$IMAGE" | cut -d' ' -f1)"
if [ "$PRIVATE_KWIN" = "1" ]; then
  [ -s "$NATIVE_IMAGE" ] || fail "native current-window PNG is missing"
  NATIVE_DESCRIPTION="$(timeout --kill-after=2s 5s file "$NATIVE_IMAGE" || true)"
  NATIVE_DIMS="$(printf '%s\n' "$NATIVE_DESCRIPTION" | sed -nE 's/.*PNG image data, ([0-9]+) x ([0-9]+).*/\1 \2/p')"
  read -r NATIVE_WIDTH NATIVE_HEIGHT <<<"${NATIVE_DIMS:-0 0}"
  NATIVE_WIDTH="${NATIVE_WIDTH:-0}"
  NATIVE_HEIGHT="${NATIVE_HEIGHT:-0}"
  NATIVE_BYTES="$(stat -c%s "$NATIVE_IMAGE")"
  NATIVE_SHA256="$(sha256sum "$NATIVE_IMAGE" | cut -d' ' -f1)"
  [ "$NATIVE_WIDTH" -eq "$IMAGE_WIDTH" ] || fail "native PNG width differs from active-window PNG"
  [ "$NATIVE_HEIGHT" -eq "$IMAGE_HEIGHT" ] || fail "native PNG height differs from active-window PNG"
fi
LOG_SHA256="$(sha256sum "$LOG" | cut -d' ' -f1)"
MARKERS_SHA256="$(sha256sum "$MARKERS" | cut -d' ' -f1)"

native_metadata=""
if [ "$PRIVATE_KWIN" = "1" ]; then
  native_metadata="$(printf 'native_image=native-window.png\nnative_width=%s\nnative_height=%s\nnative_bytes=%s\nnative_sha256=%s\nwindow_info=window-info.txt\nwindow_info_sha256=%s\nkwin_display=%s' \
    "$NATIVE_WIDTH" "$NATIVE_HEIGHT" "$NATIVE_BYTES" "$NATIVE_SHA256" \
    "$WINDOW_INFO_SHA256" "$KWIN_DISPLAY")"
fi

cat >"$METADATA" <<EOF
schema_version=1
run_id=$RUN_ID
run_uuid=$CAPTURE_RUN_UUID
frontend=host-wayland
run_root=$RUN_RELATIVE
runtime_root=$CAPTURE_RUNTIME_ROOT
supervisor_pid=${TM_CAPTURE_SUPERVISOR_PID:-}
cgroup_path=${TM_CAPTURE_CGROUP_PATH:-}
captured_at=$CAPTURED_AT
git_head=$GIT_HEAD
worktree=$WORKTREE_STATE
rust=$RUST_VERSION
binary=$APP_RELATIVE
binary_sha256=$BINARY_SHA256
build_command=cargo build --locked --quiet -p taskmanager-gpui --bin taskforest-g
build_status=success
app_pid=$APP_PID
app_pid_start_time=$APP_START_TIME
app_exe=$APP_EXE
app_pid_exe_verified=true
scenario=$SCENARIO
theme=$SKIN
expected_logical_size=$WINDOW_SIZE
display_scale=$DISPLAY_SCALE
display_geometry=$DISPLAY_GEOMETRY
display_mode=$DISPLAY_MODE
capture_backend=spectacle-active-window
window_identity=$([ "$PRIVATE_KWIN" = "1" ] && printf 'kwin-script-stacking-order' || printf 'active-window-selector-unverified')
runtime_isolation=$([ "$PRIVATE_KWIN" = "1" ] && printf 'private-kwin-wayland' || printf 'host-wayland-target-only')
dbus_isolation=private-session
dbus_address_sha256=$DBUS_ADDRESS_SHA256
kwin_pid=$KWIN_PID
kwin_pid_start_time=$KWIN_START_TIME
kwin_runtime=$KWIN_RUNTIME
kwin_socket=$HOST_XDG/$HOST_DISPLAY
tmpdir=$TMPDIR
cargo_target_dir=$CARGO_TARGET_DIR
image=host.png
image_width=$IMAGE_WIDTH
image_height=$IMAGE_HEIGHT
image_bytes=$IMAGE_BYTES
image_sha256=$IMAGE_SHA256
source_manifest=$RUN_RELATIVE/gpui-source-manifest.sha256
source_manifest_sha256=$SOURCE_MANIFEST_SHA256
$native_metadata
log=app.log
log_sha256=$LOG_SHA256
markers=markers.log
markers_sha256=$MARKERS_SHA256
parity_evidence=false
durable_output=none
EOF

if ! PYTHONDONTWRITEBYTECODE=1 timeout --kill-after=5s 30s python3 \
  "$REPO/scripts/validate_host_wayland_diagnostic.py" \
  --image "$IMAGE" --metadata "$METADATA" --markers "$MARKERS" --ocr-output "$OCR" \
  --receipt "$RECEIPT" --run-dir "$RUN_DIR" --repo-root "$REPO" \
  --logical-size "$WINDOW_SIZE" --scenario "$SCENARIO" --theme "$SKIN" --current-worktree; then
  fail "independent host diagnostic validator rejected the capture"
fi

HOLD_SECONDS="${TM_CAPTURE_ISOLATION_HOLD_SECONDS:-0}"
case "$HOLD_SECONDS" in
  ''|*[!0-9]*) fail "TM_CAPTURE_ISOLATION_HOLD_SECONDS must be a non-negative integer" ;;
esac
if [ "$HOLD_SECONDS" -gt 0 ]; then
  sleep "$HOLD_SECONDS"
fi

printf 'diagnostic_only=true\nparity_evidence=false\ndurable_output=none\n' >"$RUN_DIR/classification.txt"
printf 'HOST WAYLAND DIAGNOSTIC PASS (not parity evidence): %s\n' "$RUN_DIR"
