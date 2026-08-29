#!/usr/bin/env bash
# Capture the Iced Performance matrix in one owned nested-Niri session.
#
# This is intentionally shaped like capture-niri.sh: build once, start one
# private compositor, launch one scenario at a time, bind each screenshot to
# its exact PID/app-id/window-id, and validate the complete receipt at the end.
# It never starts one compositor per device and never publishes partial Iced
# evidence into the public documentation tree. By default the nested Niri runs
# inside a private virtual KWin host, so the operator's desktop is untouched;
# set TM_CAPTURE_NIRI_BACKGROUND=0 only for visible compositor debugging.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"
eval "$(scripts/agent-workdir.sh enter iced-capture-matrix)"
export RUSTC_WRAPPER=

APP="$REPO/target/debug/taskforest-i"
EVIDENCE_ROOT="$REPO/target/iced-evidence"
CANONICAL_MATRIX="$REPO/scripts/capture_iced_scenarios.tsv"
RUN_STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
GIT_HEAD="$(git rev-parse --short=12 HEAD 2>/dev/null || printf 'no-git')"
if [ -n "$(git status --porcelain 2>/dev/null)" ]; then
  WORKTREE_STATE=dirty
else
  WORKTREE_STATE=clean
fi
RUN_ID="${RUN_STAMP}_${GIT_HEAD}_${WORKTREE_STATE}_$$"
RUN_DIR="$EVIDENCE_ROOT/$RUN_ID"
# Keep Niri's IPC and Wayland socket paths below Linux's sockaddr_un limit even
# when the checkout itself lives under a long mounted workspace path.
RUNTIME_DIR="$(mktemp -d /tmp/taskforest-iced-niri.XXXXXX)"
CONF="$RUN_DIR/config.kdl"
METADATA="$RUN_DIR/iced-capture-metadata.txt"
SOURCE_MANIFEST="$RUN_DIR/iced-source-manifest.sha256"
NIRI_OUTPUTS="$RUN_DIR/niri-outputs.json"
MANIFEST="$RUN_DIR/iced-capture-manifest.tsv"
MATRIX_RECEIPT="$RUN_DIR/iced-capture-validation.json"
HOST_XDG="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
HOST_DISPLAY="$HOST_XDG/${WAYLAND_DISPLAY:-wayland-0}"
APP_ID="io.github.YellowWhiteBlackCat.TaskForestI"
CAPTURE_NIRI_BACKGROUND="${TM_CAPTURE_NIRI_BACKGROUND:-1}"
NIRI_PID=""
NIRI_PGID=""
APP_PID=""
APP_PGID=""
KWIN_PID=""
KWIN_PGID=""
KWIN_RUNTIME=""
KWIN_DISPLAY=""
NIRI_PARENT_WAYLAND="$HOST_DISPLAY"

mkdir -p "$RUN_DIR"
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
  terminate_owned "$APP_PID" "$APP_PGID"
  terminate_owned "$NIRI_PID" "$NIRI_PGID"
  terminate_owned "$KWIN_PID" "$KWIN_PGID"
  if [ -n "$KWIN_RUNTIME" ] && [ -d "$KWIN_RUNTIME" ]; then
    rm -rf -- "$KWIN_RUNTIME"
  fi
  if [ -d "$RUNTIME_DIR" ]; then
    rm -rf -- "$RUNTIME_DIR"
  fi
  if [ -n "${TASKMGR_AGENT_LEASE:-}" ] && [ -d "$TASKMGR_AGENT_LEASE" ]; then
    rm -rf -- "$TASKMGR_AGENT_LEASE"
  fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

for command in cargo file git jq niri rustc sha256sum setsid stat timeout; do
  command -v "$command" >/dev/null 2>&1 || {
    printf 'required capture command is unavailable: %s\n' "$command" >&2
    exit 2
  }
done
case "$CAPTURE_NIRI_BACKGROUND" in
  0|1) ;;
  *)
    printf 'TM_CAPTURE_NIRI_BACKGROUND must be 0 or 1: %s\n' \
      "$CAPTURE_NIRI_BACKGROUND" >&2
    exit 2
    ;;
esac
if [ "$CAPTURE_NIRI_BACKGROUND" -eq 1 ] \
  && ! command -v kwin_wayland >/dev/null 2>&1; then
  printf 'background Iced capture requires kwin_wayland --virtual; set TM_CAPTURE_NIRI_BACKGROUND=0 for visible nested debug mode\n' >&2
  exit 2
fi

MATRIX="$CANONICAL_MATRIX"
CAPTURE_SCOPE=full-matrix
if [ -n "${TM_ICED_CAPTURE_SCENARIOS:-}" ]; then
  MATRIX="$RUN_DIR/capture-scenarios.tsv"
  awk -F '\t' -v list="$TM_ICED_CAPTURE_SCENARIOS" '
    BEGIN {
      count = split(list, names, ",")
      for (i = 1; i <= count; i++) wanted[names[i]] = 1
    }
    NR == 1 || wanted[$1] { print }
  ' "$CANONICAL_MATRIX" >"$MATRIX"
  [ "$(wc -l <"$MATRIX")" -gt 1 ] || {
    printf 'no Iced scenarios matched TM_ICED_CAPTURE_SCENARIOS=%s\n' \
      "$TM_ICED_CAPTURE_SCENARIOS" >&2
    exit 2
  }
  CAPTURE_SCOPE=targeted
elif [ -n "${TM_ICED_CAPTURE_DEVICES:-}" ]; then
  MATRIX="$RUN_DIR/capture-devices.tsv"
  awk -F '\t' -v list="$TM_ICED_CAPTURE_DEVICES" '
    BEGIN {
      count = split(list, names, ",")
      for (i = 1; i <= count; i++) wanted[names[i]] = 1
    }
    NR == 1 || wanted[$1] || wanted[$2] { print }
  ' "$CANONICAL_MATRIX" >"$MATRIX"
  [ "$(wc -l <"$MATRIX")" -gt 1 ] || {
    printf 'no Iced device scenarios matched TM_ICED_CAPTURE_DEVICES=%s\n' \
      "$TM_ICED_CAPTURE_DEVICES" >&2
    exit 2
  }
  CAPTURE_SCOPE=targeted
elif [ -n "${TM_ICED_CAPTURE_DEVICE:-}" ]; then
  MATRIX="$RUN_DIR/capture-device.tsv"
  awk -F '\t' -v wanted="$TM_ICED_CAPTURE_DEVICE" \
    'NR == 1 || $1 == wanted || $2 == wanted { print }' \
    "$CANONICAL_MATRIX" >"$MATRIX"
  [ "$(wc -l <"$MATRIX")" -gt 1 ] || {
    printf 'unsupported TM_ICED_CAPTURE_DEVICE=%s\n' \
      "$TM_ICED_CAPTURE_DEVICE" >&2
    exit 2
  }
  CAPTURE_SCOPE=targeted
elif [ -n "${TM_ICED_CAPTURE_SOURCE_FAILURE:-}" ]; then
  case "$TM_ICED_CAPTURE_SOURCE_FAILURE" in
    services|startup|users) ;;
    *)
      printf 'unsupported TM_ICED_CAPTURE_SOURCE_FAILURE=%s\n' \
        "$TM_ICED_CAPTURE_SOURCE_FAILURE" >&2
      exit 2
      ;;
  esac
  MATRIX="$RUN_DIR/capture-source-failure.tsv"
  {
    printf 'name\tdevice\twindow_size\n'
    printf 'source-%s\t%s\t1180x780\n' \
      "$TM_ICED_CAPTURE_SOURCE_FAILURE" "$TM_ICED_CAPTURE_SOURCE_FAILURE"
  } >"$MATRIX"
  CAPTURE_SCOPE=targeted-source-failure
fi

git status --short >"$RUN_DIR/git-status.txt"
git diff --binary HEAD >"$RUN_DIR/worktree.diff"
PYTHONDONTWRITEBYTECODE=1 timeout 60s python3 scripts/frontend_source_manifest.py \
  --frontend iced --repo-root "$REPO" --output "$SOURCE_MANIFEST"
SOURCE_MANIFEST_SHA256="$(sha256sum "$SOURCE_MANIFEST" | cut -d' ' -f1)"

MATRIX_COUNT="$(awk -F '\t' 'NR > 1 && $1 != "" { count += 1 } END { print count + 0 }' "$MATRIX")"
[ "$MATRIX_COUNT" -gt 0 ] || {
  printf 'Iced capture matrix is empty: %s\n' "$MATRIX" >&2
  exit 2
}

timeout --kill-after=10s 20m cargo build --locked --quiet \
  --no-default-features --features ui-iced
install -Dm755 "$REPO/target/debug/taskmanager" "$APP"
[ -x "$APP" ] || {
  printf 'Iced binary was not produced: %s\n' "$APP" >&2
  exit 1
}
BINARY_SHA256="$(sha256sum "$APP" | cut -d' ' -f1)"

CAPTURED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
{
  printf 'run_id=%s\n' "$RUN_ID"
  printf 'captured_at=%s\n' "$CAPTURED_AT"
  printf 'git_head=%s\n' "$GIT_HEAD"
  printf 'worktree=%s\n' "$WORKTREE_STATE"
  printf 'rust=%s\n' "$(rustc -V)"
  printf 'niri=%s\n' "$(niri --version)"
  printf 'binary=target/debug/taskforest-i (ui-iced shape)\n'
  printf 'binary_sha256=%s\n' "$BINARY_SHA256"
  printf 'app_id=%s\n' "$APP_ID"
  printf 'capture_scope=%s\n' "$CAPTURE_SCOPE"
  printf 'matrix=scripts/capture_iced_scenarios.tsv\n'
  printf 'scenario_count=%s\n' "$MATRIX_COUNT"
  printf 'nested_output_logical=%s\n' 'pending'
  printf 'source_scope=iced\n'
  printf 'source_manifest_sha256=%s\n' "$SOURCE_MANIFEST_SHA256"
  if [ "$CAPTURE_NIRI_BACKGROUND" -eq 1 ]; then
    printf 'niri_host=kwin-wayland-virtual\n'
  else
    printf 'niri_host=host-wayland-visible\n'
  fi
  printf 'niri_background=%s\n' "$CAPTURE_NIRI_BACKGROUND"
  printf 'command=bash scripts/capture-iced.sh\n'
} >"$METADATA"
printf 'scenario\tdevice\trequested_window\timage\tmarkers\twindows\taction\tapp_pid\twindow_id\twidth\theight\tbytes\tsha256\tstatus\n' >"$MANIFEST"

cat >"$CONF" <<KDL
screenshot-path "$RUNTIME_DIR/shot-%Y-%m-%d-%H-%M-%S.png"
hotkey-overlay {
    skip-at-startup
}
output "winit" {
    scale 1
}
window-rule {
    match app-id="$APP_ID"
    open-floating true
}
KDL
timeout 10s niri validate --config "$CONF"

start_capture_host() {
  if [ "$CAPTURE_NIRI_BACKGROUND" -eq 0 ]; then
    return 0
  fi

  # Niri's nested winit backend is a host window. Put that backend inside a
  # private virtual KWin compositor so no capture window can steal focus or
  # paint over the operator's current desktop.
  # Keep the socket path below Linux's sockaddr_un limit even when the
  # checkout itself lives under a long mounted workspace path.
  KWIN_RUNTIME="$(mktemp -d /tmp/taskforest-iced-kwin.XXXXXX)"
  KWIN_DISPLAY="$KWIN_RUNTIME/wayland-outer"
  mkdir -p "$KWIN_RUNTIME"
  chmod 700 "$KWIN_RUNTIME"
  local kwin_log="$RUN_DIR/kwin-wayland.log"
  XDG_RUNTIME_DIR="$KWIN_RUNTIME" WAYLAND_DISPLAY= DISPLAY= \
    QT_QPA_PLATFORM=wayland setsid timeout --foreground --kill-after=10s 20m \
    kwin_wayland --virtual --socket=wayland-outer --width=1920 --height=1080 \
      --scale=1 --no-global-shortcuts --no-lockscreen \
      &>"$kwin_log" & KWIN_PID=$!
  KWIN_PGID=""
  local i
  for i in $(seq 1 40); do
    KWIN_PGID="$(process_group "$KWIN_PID")"
    [ "$KWIN_PGID" = "$KWIN_PID" ] && break
    sleep 0.1
  done
  if [ "$KWIN_PGID" != "$KWIN_PID" ]; then
    printf 'virtual KWin did not obtain a private process group; evidence retained at %s\n' \
      "$RUN_DIR" >&2
    return 1
  fi
  for i in $(seq 1 40); do
    [ -S "$KWIN_DISPLAY" ] && break
    sleep 0.2
  done
  if ! kill -0 "$KWIN_PID" 2>/dev/null || [ ! -S "$KWIN_DISPLAY" ]; then
    printf 'virtual KWin did not start; tail of log:\n' >&2
    tail -20 "$kwin_log" >&2
    return 1
  fi
  NIRI_PARENT_WAYLAND="$KWIN_DISPLAY"
  printf 'background capture host: kwin-wayland --virtual (%s)\n' "$KWIN_DISPLAY"
}

start_capture_host || exit 1

XDG_RUNTIME_DIR="$RUNTIME_DIR" WAYLAND_DISPLAY="$NIRI_PARENT_WAYLAND" \
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

NIRI_OUTPUTS_TMP="$RUN_DIR/niri-outputs.json.tmp"
OUTPUT_READY=0
for _ in $(seq 1 30); do
  if NIRI_SOCKET="$IPC" timeout 1s niri msg -j outputs >"$NIRI_OUTPUTS_TMP" 2>/dev/null \
    && jq -e '((type == "object" and (.winit | type == "object") and .winit.name == "winit" and .winit.logical.scale == 1) or (type == "array" and length == 1 and .[0].name == "winit" and .[0].logical.scale == 1))' \
      "$NIRI_OUTPUTS_TMP" >/dev/null 2>&1; then
    mv "$NIRI_OUTPUTS_TMP" "$NIRI_OUTPUTS"
    OUTPUT_READY=1
    break
  fi
  sleep 0.5
done
[ "$OUTPUT_READY" -eq 1 ] || {
  printf 'nested Niri output receipt failed; evidence retained at %s\n' "$RUN_DIR" >&2
  exit 1
}
NIRI_OUTPUT_LOGICAL="$(jq -r 'if type == "object" then .winit.logical else .[0].logical end as $logical | if ($logical.width and $logical.height) then "\($logical.width)x\($logical.height)" else empty end' "$NIRI_OUTPUTS")"
[ -n "$NIRI_OUTPUT_LOGICAL" ] || {
  printf 'nested Niri output has no logical dimensions\n' >&2
  exit 1
}
sed -i "s/^nested_output_logical=.*/nested_output_logical=$NIRI_OUTPUT_LOGICAL/" "$METADATA"

capture_one() {
  local name="$1" device="$2" window_size="$3"
  # A "-zh" scenario-name suffix renders the localized surface (ICED-002
  # geometry evidence): the launch reads XDG_CONFIG_HOME/taskmanager/config.json
  # (JSON, serde), whose shared language token is "en"/"zh".
  local locale=""
  case "$name" in
  *-zh) locale="zh" ;;
  esac
  local scenario_dir="$RUN_DIR/$name"
  local log="$scenario_dir/app.log"
  local markers="$scenario_dir/markers.log"
  local windows="$scenario_dir/window.json"
  local windows_tmp="$scenario_dir/window.json.tmp"
  local windows_error="$scenario_dir/window.err"
  local action="$scenario_dir/action.log"
  local image="$scenario_dir/image.png"
  local config_home="$RUNTIME_DIR/config-$name"
  local window_id="" window_ready=0 marker_ready=0 action_status=failed
  local width=0 height=0 bytes=0 hash=-
  local page=performance
  case "$device" in
  applications|services|startup|users|system|app-history) page="$device" ;;
  service-details) page=services ;;
  esac
  mkdir -p "$scenario_dir" "$config_home"

  # Two attempts per scenario: the nested-Wayland session occasionally shows
  # late-session resource flakiness (a launch that produces no window and no
  # log, ICED-002 ledger note). Each attempt tears the previous app down and
  # starts clean; the manifest and marker receipts are written exactly once
  # from the final attempt so validation still sees one row per scenario.
  local attempt=0
  while [ "$attempt" -lt 2 ]; do
    attempt=$((attempt + 1))
    if [ "$attempt" -gt 1 ]; then
      printf '  retry %-13s (attempt %d/2)\n' "$name" "$attempt"
    fi
    rm -f "$markers" "$windows" "$windows_tmp" "$windows_error" "$action" "$image"
    terminate_owned "$APP_PID" "$APP_PGID"
    APP_PID=""
    APP_PGID=""
    window_id=""
    window_ready=0
    marker_ready=0
    action_status=failed
    width=0
    height=0
    bytes=0
    hash="-"

    printf '  start %-13s device=%-7s window=%s\n' "$name" "$device" "$window_size"
    XDG_RUNTIME_DIR="$RUNTIME_DIR" XDG_CONFIG_HOME="$config_home" \
      WAYLAND_DISPLAY="$SOCK" TM_ICED_CAPTURE_MARKER_FILE="$markers" \
      TM_ICED_CAPTURE_DEVICE="$device" TM_ICED_WINDOW_SIZE="$window_size" \
      TM_ICED_CAPTURE_LOCALE="$locale" \
      LIBGL_ALWAYS_SOFTWARE=1 setsid "$APP" --demo >"$log" 2>&1 &
    APP_PID=$!
    APP_PGID="$(process_group "$APP_PID")"

    if [ "$APP_PGID" = "$APP_PID" ]; then
      for _ in $(seq 1 80); do
        if NIRI_SOCKET="$IPC" timeout 3s niri msg -j windows >"$windows_tmp" 2>"$windows_error" \
          && jq -e --arg app "$APP_ID" --arg pid "$APP_PID" \
            'any(.[]; .app_id == $app and ((.pid | tostring) == $pid))' \
            "$windows_tmp" >/dev/null 2>&1; then
          mv "$windows_tmp" "$windows"
          window_id="$(jq -r --arg app "$APP_ID" --arg pid "$APP_PID" \
            '.[] | select(.app_id == $app and ((.pid | tostring) == $pid)) | .id' \
            "$windows" | head -1)"
          window_ready=1
          break
        fi
        sleep 0.25
      done

      for _ in $(seq 1 120); do
        if grep -q "ICED_CAPTURE_MARKER event=frame_ready mode=demo page=$page" "$markers" 2>/dev/null \
          && grep -q "ICED_CAPTURE_MARKER event=target_ready mode=demo page=$page device=$device" "$markers" 2>/dev/null; then
          marker_ready=1
          break
        fi
        kill -0 "$APP_PID" 2>/dev/null || break
        sleep 0.25
      done
      sleep 0.5
    fi

    {
      printf 'app_pid=%s\n' "$APP_PID"
      printf 'window_id=%s\n' "$window_id"
      printf 'requested_window=%s\n' "$window_size"
      printf 'command=niri msg action screenshot-window --id %s --write-to-disk true --path %s\n' \
        "$window_id" "$image"
    } >"$action"

    if [ "$window_ready" -eq 1 ] && [ "$marker_ready" -eq 1 ] \
      && kill -0 "$APP_PID" 2>/dev/null \
      && NIRI_SOCKET="$IPC" timeout 3s niri msg -j windows >"$windows_tmp" 2>"$windows_error" \
      && jq -e --arg app "$APP_ID" --arg pid "$APP_PID" --arg id "$window_id" \
        'any(.[]; .app_id == $app and ((.pid | tostring) == $pid) and ((.id | tostring) == $id))' \
        "$windows_tmp" >/dev/null 2>&1; then
      mv "$windows_tmp" "$windows"
      for _ in $(seq 1 5); do
        rm -f "$image"
        if NIRI_SOCKET="$IPC" timeout 8s niri msg action screenshot-window \
          --id "$window_id" --write-to-disk true --path "$image" >>"$action" 2>&1; then
          action_status=ok
          break
        fi
        sleep 0.25
      done
    fi

    if [ "$action_status" = ok ]; then
      for _ in $(seq 1 20); do
        if [ -s "$image" ]; then
          bytes="$(stat -c%s "$image")"
          [ "$bytes" -gt 5000 ] && break
        fi
        sleep 0.1
      done
      if [ "$bytes" -gt 5000 ]; then
        local dimensions
        dimensions="$(file "$image" | sed -nE 's/.*PNG image data, ([0-9]+) x ([0-9]+).*/\1 \2/p')"
        read -r width height <<<"$dimensions"
        if [ -n "$width" ] && [ -n "$height" ]; then
          hash="$(sha256sum "$image" | cut -d' ' -f1)"
        else
          action_status=failed
        fi
      else
        action_status=failed
      fi
    fi

    [ "$action_status" = ok ] && break
  done

  if [ "$action_status" = ok ]; then
    printf '  pass  %-13s %sx%s %s B\n' "$name" "$width" "$height" "$bytes"
  else
    printf '  FAIL  %-13s markers=%s window=%s action=%s (see %s)\n' \
      "$name" "$marker_ready" "$window_ready" "$action_status" "$log" >&2
  fi
  grep 'ICED_CAPTURE_MARKER' "$markers" 2>/dev/null | sed "s/^/$name\t/" \
    >>"$RUN_DIR/iced-capture-markers.log" || true
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$name" "$device" "$window_size" \
    "$name/image.png" "$name/markers.log" "$name/window.json" "$name/action.log" \
    "$APP_PID" "$window_id" "$width" "$height" "$bytes" "$hash" "$action_status" \
    >>"$MANIFEST"

  terminate_owned "$APP_PID" "$APP_PGID"
  APP_PID=""
  APP_PGID=""
  [ "$action_status" = ok ]
}

: >"$RUN_DIR/iced-capture-markers.log"
FAILURES=0
while IFS=$'\t' read -r name device window_size; do
  [ "$name" = name ] && continue
  [ -n "$name" ] || continue
  if ! capture_one "$name" "$device" "$window_size"; then
    FAILURES=$((FAILURES + 1))
  fi
done <"$MATRIX"

if [ "$FAILURES" -ne 0 ]; then
  printf 'Iced matrix failed: %s scenario(s); evidence retained at %s\n' \
    "$FAILURES" "$RUN_DIR" >&2
  exit 1
fi

PYTHONDONTWRITEBYTECODE=1 timeout 30s python3 scripts/validate_iced_matrix.py \
  --matrix "$MATRIX" \
  --manifest "$MANIFEST" \
  --run-dir "$RUN_DIR" \
  --metadata "$METADATA" \
  --source-manifest "$SOURCE_MANIFEST" \
  --niri-outputs "$NIRI_OUTPUTS" \
  --receipt "$MATRIX_RECEIPT" \
  --repo-root "$REPO" \
  --binary "$APP" \
  --current-worktree

printf 'ICED sequential matrix: PASS -> %s\n' "$RUN_DIR"
