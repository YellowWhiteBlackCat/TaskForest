#!/usr/bin/env bash
# Run one current-build GPUI capture against the host Wayland compositor.
#
# This is a diagnostic path, never a parity publisher. It deliberately uses
# KWin/Spectacle's active-window capture so host scale and "still collecting"
# failures can be observed without pretending that the result is a nested-Niri
# receipt. Every artifact stays below target/host-wayland-diagnostic/.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
EVIDENCE_ROOT="$REPO/target/host-wayland-diagnostic"
APP="$REPO/target/debug/taskforest-g"
SCENARIO="${TM_HOST_WAYLAND_SCENARIO:-standard}"
SKIN="${TM_HOST_WAYLAND_SKIN:-gnome-light}"
PAGE="${TM_HOST_WAYLAND_PAGE:-performance}"
DEVICE="${TM_HOST_WAYLAND_DEVICE:-cpu}"
SETTINGS="${TM_HOST_WAYLAND_SETTINGS:-0}"
WINDOW_SIZE="${TM_HOST_WAYLAND_WINDOW_SIZE:-1180x780}"

usage() {
  cat <<'USAGE'
Usage: bash scripts/capture-host-wayland-diagnostic.sh [SCENARIO]

Runs one non-publishing active-window diagnostic on the existing host Wayland
session. The image and receipts are written only below
target/host-wayland-diagnostic/. A scale other than 1, missing strict markers,
an OCR-detected telemetry skeleton, a stale build, or any receipt mismatch is
an error. No result from this script is durable parity evidence.

Environment overrides: TM_HOST_WAYLAND_{SKIN,PAGE,DEVICE,SETTINGS,WINDOW_SIZE}.
The optional positional SCENARIO overrides TM_HOST_WAYLAND_SCENARIO.
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
RUN_ID="${RUN_STAMP}_${GIT_HEAD}_${WORKTREE_STATE}"
RUN_DIR="$EVIDENCE_ROOT/$RUN_ID"
LOG="$RUN_DIR/app.log"
MARKERS="$RUN_DIR/markers.log"
METADATA="$RUN_DIR/metadata.txt"
IMAGE="$RUN_DIR/host.png"
OCR="$RUN_DIR/ocr.txt"
RECEIPT="$RUN_DIR/validation.json"
DISPLAY_RAW="$RUN_DIR/display-raw.txt"
DISPLAY_NORMALIZED="$RUN_DIR/display-normalized.txt"
SCRATCH=""
APP_PID=""
AGENT_LEASE=""

mkdir -p "$RUN_DIR"
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

[ "${XDG_SESSION_TYPE:-}" = "wayland" ] || fail "host diagnostic requires XDG_SESSION_TYPE=wayland"
[ -S "$HOST_XDG/$HOST_DISPLAY" ] || fail "host Wayland socket is unavailable: $HOST_XDG/$HOST_DISPLAY"
for command_name in cargo timeout spectacle kscreen-doctor file sha256sum; do
  command -v "$command_name" >/dev/null 2>&1 || fail "required command is unavailable: $command_name"
done

RUST_VERSION="$(rustc -V 2>/dev/null || printf 'unavailable')"
CAPTURED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# The repository's normal Cargo config may enable sccache. Its Unix socket can
# exceed SUN_LEN when this runner is launched from a long agent lease path, so
# the diagnostic build deliberately disables the wrapper while retaining the
# exact locked current-worktree command recorded in the receipt.
if ! (cd "$REPO" && RUSTC_WRAPPER= timeout --kill-after=10s 20m cargo build --locked --quiet); then
  fail "locked current-worktree build failed"
fi
install -Dm755 "$REPO/target/debug/taskmanager" "$APP"
[ -x "$APP" ] || fail "current build did not produce $APP"
BINARY_SHA256="$(sha256sum "$APP" | cut -d' ' -f1)"

if ! timeout --kill-after=3s 10s kscreen-doctor -o >"$DISPLAY_RAW" 2>&1; then
  fail "kscreen-doctor could not provide a display scale receipt"
fi
sed -E 's/\x1B\[[0-9;]*m//g' "$DISPLAY_RAW" >"$DISPLAY_NORMALIZED"
DISPLAY_SCALE="$(sed -nE 's/^[[:space:]]*Scale:[[:space:]]*([0-9]+([.][0-9]+)?).*/\1/p' "$DISPLAY_NORMALIZED" | head -1)"
DISPLAY_GEOMETRY="$(sed -nE 's/^[[:space:]]*Geometry:[[:space:]]*(.*)/\1/p' "$DISPLAY_NORMALIZED" | head -1 | tr '\t' ' ')"
DISPLAY_MODE="$(sed -nE 's/^[[:space:]]*Modes:[[:space:]]*(.*)/\1/p' "$DISPLAY_NORMALIZED" | head -1 | tr '\t' ' ')"
[ -n "$DISPLAY_SCALE" ] || fail "display scale was absent from kscreen-doctor receipt"
[ -n "$DISPLAY_GEOMETRY" ] || fail "display geometry was absent from kscreen-doctor receipt"

mkdir -p "$RUN_DIR/config" "$RUN_DIR/state" "$RUN_DIR/data" "$RUN_DIR/cache"
TM_CAPTURE_SCENARIO="$SCENARIO"
[ "$SCENARIO" = standard ] && TM_CAPTURE_SCENARIO=""
XDG_RUNTIME_DIR="$HOST_XDG" XDG_CONFIG_HOME="$RUN_DIR/config" \
  XDG_STATE_HOME="$RUN_DIR/state" XDG_DATA_HOME="$RUN_DIR/data" XDG_CACHE_HOME="$RUN_DIR/cache" \
  WAYLAND_DISPLAY="$HOST_DISPLAY" WINIT_UNIX_BACKEND=wayland \
  LANG=C.UTF-8 LANGUAGE=en_US:en LC_ALL=C.UTF-8 \
  TM_SKIN="$SKIN" TM_PAGE="$PAGE" TM_DEVICE="$DEVICE" TM_SETTINGS="$SETTINGS" \
  TM_SKIN_HC="" TM_CAPTURE_EVIDENCE=1 TM_CAPTURE_SCENARIO="$TM_CAPTURE_SCENARIO" \
  TM_WINDOW_SIZE="$WINDOW_SIZE" \
  "$APP" &>"$LOG" &
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

# Give the marker-triggered frame a bounded settle interval. The validator's
# OCR and pixel checks still reject a stable "Collecting telemetry…" skeleton.
sleep 2
if ! kill -0 "$APP_PID" 2>/dev/null; then
  fail "current-build app exited before active-window capture"
fi
if [ "$(readlink -f "/proc/$APP_PID/exe" 2>/dev/null || true)" != "$APP" ]; then
  fail "app PID ownership changed before capture"
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
LOG_SHA256="$(sha256sum "$LOG" | cut -d' ' -f1)"
MARKERS_SHA256="$(sha256sum "$MARKERS" | cut -d' ' -f1)"

cat >"$METADATA" <<EOF
schema_version=1
run_id=$RUN_ID
captured_at=$CAPTURED_AT
git_head=$GIT_HEAD
worktree=$WORKTREE_STATE
rust=$RUST_VERSION
binary=target/debug/taskforest-g
binary_sha256=$BINARY_SHA256
build_command=cargo build --locked --quiet
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
window_identity=active-window-selector-unverified
runtime_isolation=host-wayland-target-only
tmpdir=$TMPDIR
cargo_target_dir=$CARGO_TARGET_DIR
image=host.png
image_width=$IMAGE_WIDTH
image_height=$IMAGE_HEIGHT
image_bytes=$IMAGE_BYTES
image_sha256=$IMAGE_SHA256
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

printf 'diagnostic_only=true\nparity_evidence=false\ndurable_output=none\n' >"$RUN_DIR/classification.txt"
printf 'HOST WAYLAND DIAGNOSTIC PASS (not parity evidence): %s\n' "$RUN_DIR"
