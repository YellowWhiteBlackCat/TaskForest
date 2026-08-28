#!/bin/bash
# Capture screenshots of the app by running it nested inside Niri (a Rust/Smithay
# compositor) and using niri's built-in screenshot-window IPC action.
#
# Why nested Niri: Plasma's portal won't allow non-interactive capture, and gpui
# 0.2.2 has no in-app pixel readback (the test harness renders a Scene, not pixels).
# Niri run nested under the host session exposes its own Wayland socket and captures
# via its compositor-level screenshot action — fully autonomous, no portal, no sudo,
# no system files.
#
# niri 26.04 notes (adapted forward, not downgraded):
#   * Disable the nested compositor's "Important Hotkeys" prompt with the canonical
#     multi-line `hotkey-overlay { skip-at-startup }` block. The compact one-line
#     form is not valid KDL for this node, so validate the generated config before
#     starting Niri.
#   * niri msg no longer takes --socket; pass the IPC socket via NIRI_SOCKET env.
#   * screenshot-window is called with an explicit window id from the current
#     app PID/app_id receipt. This avoids depending on focus while retaining the
#     compositor-level capture path (unlike grim/wlr-screencopy, which waits for
#     the next frame and can hang when the compositor has no fresh damage).
#   * Every run gets a private XDG runtime directory. Nested Wayland/IPC sockets and
#     exact child PIDs are therefore owned by this script without global `pkill`.
#
# Prereqs: cargo + niri + jq + kwin_wayland + setsid + file + sha256sum;
# magick/montage is optional for the contact sheet. Set
# TM_CAPTURE_NIRI_BACKGROUND=0 only for an explicit visible-host debug run.
# The script always rebuilds the current locked worktree before capture.
# Usage: bash scripts/capture-niri.sh
set -u
REPO="$(cd "$(dirname "$0")/.." && pwd)"
APP="$REPO/target/debug/taskforest-g"
APP_ID="io.github.YellowWhiteBlackCat.TaskForestG"
HOST_XDG="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
HOST_DISPLAY="$HOST_XDG/${WAYLAND_DISPLAY:-wayland-0}"
# The nested winit backend is sensitive to host GPU/frame pacing while a client
# maps. Keep capture deterministic and isolate that variable by default. Set
# TM_CAPTURE_LIBGL_ALWAYS_SOFTWARE=0 only for an explicit hardware-driver probe.
CAPTURE_LIBGL_ALWAYS_SOFTWARE="${TM_CAPTURE_LIBGL_ALWAYS_SOFTWARE:-1}"
CAPTURE_NIRI_LOG="${TM_CAPTURE_NIRI_LOG:-niri=info}"
# Run the nested compositor inside a private virtual KWin framebuffer by
# default. Set this to 0 only when a visible host-wayland nested window is
# explicitly desired for manual compositor debugging.
CAPTURE_NIRI_BACKGROUND="${TM_CAPTURE_NIRI_BACKGROUND:-1}"
# A fresh nested compositor per scenario isolates Smithay/winit state from the
# previous client. Hosts that have independently qualified longer-lived Niri
# sessions may raise this value, but the fail-closed default favors complete,
# reproducible evidence over a small startup-time saving.
CAPTURE_NIRI_BATCH_SIZE="${TM_CAPTURE_NIRI_BATCH_SIZE:-1}"
CAPTURE_NIRI_MAX_ATTEMPTS="${TM_CAPTURE_NIRI_MAX_ATTEMPTS:-3}"
OUT="$REPO/target/screenshot-evidence/latest"
CANONICAL_MATRIX="$REPO/scripts/capture_scenarios.tsv"
MATRIX="$CANONICAL_MATRIX"
EVIDENCE_ROOT="$REPO/target/screenshot-evidence"
SCRATCH_ROOT="$REPO/.tmp"
RUN_STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
GIT_HEAD="$(git -C "$REPO" rev-parse --short=12 HEAD 2>/dev/null || printf 'no-git')"
if [ -n "$(git -C "$REPO" status --porcelain 2>/dev/null)" ]; then
  WORKTREE_STATE=dirty
else
  WORKTREE_STATE=clean
fi
RUN_ID="${RUN_STAMP}_${GIT_HEAD}_${WORKTREE_STATE}_$$"
RUN_DIR="$EVIDENCE_ROOT/$RUN_ID"
STAGED="$RUN_DIR/screenshots"
TMP="$RUN_DIR/runtime"
NIRI_RUNTIME=""
CONF="$RUN_DIR/config.kdl"
MANIFEST="$RUN_DIR/capture-manifest.tsv"
WINDOW_MANIFEST="$RUN_DIR/capture-window-receipts.tsv"
METADATA="$RUN_DIR/capture-metadata.txt"
MARKERS="$RUN_DIR/capture-markers.log"
VALIDATION="$RUN_DIR/capture-validation.json"
SOURCE_MANIFEST="$RUN_DIR/gpui-source-manifest.sha256"
for command in cargo file git jq niri rustc sha256sum setsid stat timeout; do
  if ! command -v "$command" >/dev/null 2>&1; then
    printf 'required capture command is unavailable: %s\n' "$command" >&2
    exit 2
  fi
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
  printf 'background Niri capture requires kwin_wayland --virtual; set TM_CAPTURE_NIRI_BACKGROUND=0 for visible nested debug mode\n' >&2
  exit 2
fi
mkdir -p "$OUT" "$STAGED" "$TMP" "$SCRATCH_ROOT"
# Niri puts its IPC socket below XDG_RUNTIME_DIR. Keep this private runtime on
# at SUN_LEN and the repository/lease path is already long. Cargo scratch and
# build output still use the agent lease supplied by the caller.
NIRI_RUNTIME="$(mktemp -d /tmp/taskforest-niri.XXXXXX)"
chmod 700 "$NIRI_RUNTIME"
NIRI_PID=""
NIRI_PGID=""
NIRI_START_COUNT=0
SOCK=""
IPC=""
APP_PID=""
APP_PGID=""
KWIN_PID=""
KWIN_PGID=""
KWIN_RUNTIME=""
KWIN_DISPLAY=""
NIRI_PARENT_WAYLAND="$HOST_DISPLAY"

case "$CAPTURE_NIRI_BATCH_SIZE" in
  ''|*[!0-9]*|0)
    printf 'TM_CAPTURE_NIRI_BATCH_SIZE must be a positive integer: %s\n' \
      "$CAPTURE_NIRI_BATCH_SIZE" >&2
    exit 2
    ;;
esac
case "$CAPTURE_NIRI_MAX_ATTEMPTS" in
  ''|*[!0-9]*|0)
    printf 'TM_CAPTURE_NIRI_MAX_ATTEMPTS must be a positive integer: %s\n' \
      "$CAPTURE_NIRI_MAX_ATTEMPTS" >&2
    exit 2
    ;;
esac

# Optional targeted review mode. The canonical/default path remains the full
# matrix; a comma-separated filter is useful for a single visual proof (e.g.
# the newly added process-selection rail) without spending 20 minutes recapturing
# unchanged scenarios. Filtered runs are deliberately non-publishing: their
# evidence stays in target/screenshot-evidence and can never replace the durable
# complete matrix with a partial manifest.
PUBLISH_CAPTURE=1
CAPTURE_SCOPE=full-matrix
if [ -n "${TM_CAPTURE_SCENARIOS:-}" ]; then
  FILTERED_MATRIX="$RUN_DIR/capture-scenarios.tsv"
  awk -F '\t' -v list="$TM_CAPTURE_SCENARIOS" '
    BEGIN {
      count = split(list, names, ",")
      for (i = 1; i <= count; i++) wanted[names[i]] = 1
    }
    NR == 1 || wanted[$1] { print }
  ' "$MATRIX" >"$FILTERED_MATRIX"
  if [ "$(wc -l <"$FILTERED_MATRIX")" -le 1 ]; then
    printf 'no capture scenarios matched TM_CAPTURE_SCENARIOS=%s\n' "$TM_CAPTURE_SCENARIOS"
    exit 2
  fi
  MATRIX="$FILTERED_MATRIX"
  PUBLISH_CAPTURE=0
  CAPTURE_SCOPE=targeted
  printf 'targeted capture mode: %s (local-only)\n' "$TM_CAPTURE_SCENARIOS"
fi

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
  # A failed run keeps its private runtime tree (per-scenario XDG config/data
  # homes) so the fixture state can be inspected post-mortem.
  if [ "${FAILURES:-0}" -gt 0 ]; then
    printf 'retaining failed-run runtime tree: %s\n' "$NIRI_RUNTIME" >&2
  else
    rm -rf "$NIRI_RUNTIME"
  fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
RUST_VERSION="$(rustc -V 2>/dev/null || printf 'unavailable')"
NIRI_VERSION="$(niri --version 2>/dev/null || printf 'unavailable')"
CAPTURED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

printf 'captured_at\tgit_head\tworktree\trust\tscenario\timage\tskin\tpage\tdevice\tsettings\twidth\theight\tbytes\tsha256\tmarkers\tlog\tlog_sha256\tmarker_receipt\n' >"$MANIFEST"
printf 'scenario\tapp_pid\twindow_id\twindows_json\twindows_sha256\taction_log\taction_sha256\n' >"$WINDOW_MANIFEST"
: >"$MARKERS"
git -C "$REPO" status --porcelain >"$RUN_DIR/git-status.txt" 2>/dev/null
git -C "$REPO" diff --binary HEAD >"$RUN_DIR/worktree.diff" 2>/dev/null
PYTHONDONTWRITEBYTECODE=1 timeout 60s python3 scripts/frontend_source_manifest.py \
  --frontend gpui --repo-root "$REPO" --output "$SOURCE_MANIFEST"
SOURCE_MANIFEST_SHA256="$(sha256sum "$SOURCE_MANIFEST" | cut -d' ' -f1)"

# Never capture a stale executable. Locked build failures leave the accepted
# screenshot set untouched and retain this run directory for diagnosis.
(cd "$REPO" && timeout --kill-after=10s 20m cargo build --locked --quiet \
  -p taskmanager) || {
  printf 'capture build failed; evidence retained at %s\n' "$RUN_DIR"
  exit 1
}
install -Dm755 "$REPO/target/debug/taskmanager" "$APP"
BINARY_SHA256="$(sha256sum "$APP" | cut -d' ' -f1)"
{
  printf 'run_id=%s\n' "$RUN_ID"
  printf 'captured_at=%s\n' "$CAPTURED_AT"
  printf 'git_head=%s\n' "$GIT_HEAD"
  printf 'worktree=%s\n' "$WORKTREE_STATE"
  printf 'rust=%s\n' "$RUST_VERSION"
  printf 'niri=%s\n' "$NIRI_VERSION"
  printf 'binary=target/debug/taskforest-g\n'
  printf 'binary_sha256=%s\n' "$BINARY_SHA256"
  printf 'capture_scope=%s\n' "$CAPTURE_SCOPE"
  printf 'publish=%s\n' "$([ "$PUBLISH_CAPTURE" -eq 1 ] && printf full-matrix || printf targeted)"
  printf 'source_scope=gpui\n'
  printf 'source_manifest_sha256=%s\n' "$SOURCE_MANIFEST_SHA256"
  printf 'libgl_always_software=%s\n' "$CAPTURE_LIBGL_ALWAYS_SOFTWARE"
  if [ "$CAPTURE_NIRI_BACKGROUND" -eq 1 ]; then
    printf 'niri_host=kwin-wayland-virtual\n'
  else
    printf 'niri_host=host-wayland-visible\n'
  fi
  printf 'niri_background=%s\n' "$CAPTURE_NIRI_BACKGROUND"
  printf 'niri_batch_size=%s\n' "$CAPTURE_NIRI_BATCH_SIZE"
  printf 'niri_max_attempts=%s\n' "$CAPTURE_NIRI_MAX_ATTEMPTS"
  printf 'niri_outputs=target/screenshot-evidence/%s/niri-outputs.json\n' "$RUN_ID"
  printf 'window_receipts=target/screenshot-evidence/%s/capture-window-receipts.tsv\n' "$RUN_ID"
  printf 'command=bash scripts/capture-niri.sh\n'
} >"$METADATA"
# Minimal valid 26.04 config: where niri saves screenshots (timestamped), and float
# the app so it respects its requested 1180x780 (landscape) instead of being tiled
# into a tall column. screenshot-window then captures just that floating window.
cat > "$CONF" <<KDL
screenshot-path "$TMP/shot-%Y-%m-%d-%H-%M-%S.png"
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
timeout 10s niri validate --config "$CONF" || {
  printf 'generated Niri config is invalid; evidence retained at %s\n' "$RUN_DIR"
  exit 1
}

NIRI_OUTPUTS="$RUN_DIR/niri-outputs.json"
NIRI_OUTPUTS_ERROR="$RUN_DIR/niri-outputs.err"
NIRI_OUTPUTS_TMP="$RUN_DIR/niri-outputs.json.tmp"
NIRI_OUTPUT_LOGICAL=""

stop_niri() {
  terminate_owned "$NIRI_PID" "$NIRI_PGID"
  NIRI_PID=""
  NIRI_PGID=""
  SOCK=""
  IPC=""
}

start_niri() {
  NIRI_START_COUNT=$((NIRI_START_COUNT + 1))
  local niri_log="$RUN_DIR/niri.log"
  if [ "$NIRI_START_COUNT" -gt 1 ]; then
    niri_log="$RUN_DIR/niri-restart-$NIRI_START_COUNT.log"
  fi

  XDG_RUNTIME_DIR="$NIRI_RUNTIME" WAYLAND_DISPLAY="$NIRI_PARENT_WAYLAND" DISPLAY= \
    LIBGL_ALWAYS_SOFTWARE="$CAPTURE_LIBGL_ALWAYS_SOFTWARE" \
    RUST_LOG="$CAPTURE_NIRI_LOG" \
    setsid timeout --foreground --kill-after=10s 20m niri --config "$CONF" \
      &>"$niri_log" & NIRI_PID=$!
  NIRI_PGID=""
  local i
  for i in $(seq 1 40); do
    NIRI_PGID="$(process_group "$NIRI_PID")"
    [ "$NIRI_PGID" = "$NIRI_PID" ] && break
    sleep 0.1
  done
  if [ "$NIRI_PGID" != "$NIRI_PID" ]; then
    printf 'nested Niri did not obtain a private process group; evidence retained at %s\n' \
      "$RUN_DIR" >&2
    return 1
  fi

  SOCK=""
  for i in $(seq 1 40); do
    SOCK="$(find "$NIRI_RUNTIME" -maxdepth 1 -type s -name 'wayland-[0-9]*' \
      -printf '%f\n' -quit 2>/dev/null || true)"
    if [ -z "$SOCK" ]; then
      SOCK="$(grep -oE 'wayland-[0-9]+' "$niri_log" | head -1 || true)"
    fi
    [ -n "$SOCK" ] && [ -S "$NIRI_RUNTIME/$SOCK" ] && break
    sleep 0.2
  done
  IPC="$(find "$NIRI_RUNTIME" -maxdepth 1 -type s -name 'niri.*.sock' \
    -print -quit 2>/dev/null || true)"
  if [ -z "$IPC" ]; then
    IPC="$(grep -oE "$NIRI_RUNTIME/niri\.[^ ]*\.sock" "$niri_log" | head -1 || true)"
  fi
  if ! kill -0 "$NIRI_PID" 2>/dev/null || [ -z "$SOCK" ] || [ -z "$IPC" ]; then
    printf 'niri did not start nested; tail of log:\n'
    tail -8 "$niri_log"
    return 1
  fi

  local output_ready=0
  for i in $(seq 1 60); do
    # Two seconds per IPC probe and a large retry budget: a live host session
    # (KWin/Plasma) can pause new nested-client setup for tens of seconds
    # after a burst of compositor start/stop cycles. A probe budget this size
    # absorbs that without mistaking a busy host for a dead compositor.
    if NIRI_SOCKET="$IPC" timeout 2s niri msg -j outputs \
      >"$NIRI_OUTPUTS_TMP" 2>"$NIRI_OUTPUTS_ERROR" \
      && jq -e '((type == "object" and (.winit | type == "object") and .winit.name == "winit" and .winit.logical.scale == 1) or (type == "array" and length == 1 and .[0].name == "winit" and .[0].logical.scale == 1))' \
        "$NIRI_OUTPUTS_TMP" >/dev/null 2>&1; then
      mv "$NIRI_OUTPUTS_TMP" "$NIRI_OUTPUTS"
      output_ready=1
      break
    fi
    sleep 0.5
  done
  if [ "$output_ready" -ne 1 ]; then
    [ -f "$NIRI_OUTPUTS_TMP" ] && cp "$NIRI_OUTPUTS_TMP" "$NIRI_OUTPUTS" || : >"$NIRI_OUTPUTS"
    printf 'nested Niri output receipt failed after %s probes; evidence retained at %s\n' \
      "$i" "$RUN_DIR" >&2
    printf 'probe stderr:\n' >&2
    cat "$NIRI_OUTPUTS_ERROR" >&2
    printf 'tail of %s:\n' "$niri_log" >&2
    tail -8 "$niri_log" >&2
    return 1
  fi
  # Let the host settle for one second after a compositor maps: the next
  # client launches immediately and back-to-back nested startups are what
  # wedged the host session in the first place.
  sleep 1

  local logical
  logical="$(jq -r 'if type == "object" then .winit.logical else .[0].logical end as $logical | if ($logical.width and $logical.height) then "\($logical.width)x\($logical.height)" else empty end' "$NIRI_OUTPUTS")"
  if [ -z "$logical" ]; then
    printf 'nested Niri output has no logical dimensions; evidence retained at %s\n' \
      "$RUN_DIR" >&2
    return 1
  fi
  if [ -n "$NIRI_OUTPUT_LOGICAL" ] && [ "$logical" != "$NIRI_OUTPUT_LOGICAL" ]; then
    printf 'nested Niri output changed across batches: %s -> %s\n' \
      "$NIRI_OUTPUT_LOGICAL" "$logical" >&2
    return 1
  fi
  if [ -z "$NIRI_OUTPUT_LOGICAL" ]; then
    NIRI_OUTPUT_LOGICAL="$logical"
    {
      printf 'nested_output_name=winit\n'
      printf 'nested_output_scale=1\n'
      printf 'nested_output_logical=%s\n' "$NIRI_OUTPUT_LOGICAL"
    } >>"$METADATA"
  fi
  printf 'niri nested batch %s on %s (ipc %s)\n' \
    "$NIRI_START_COUNT" "$SOCK" "$(basename "$IPC")"
}

ensure_niri() {
  # A live host session can refuse nested-compositor setup for minutes at a
  # time (observed: a KWin/Plasma pause after a burst of nested start/stop
  # cycles). Fail closed, but spend the budget retrying with backoff before
  # giving up the whole single-run matrix receipt.
  if start_niri; then
    return 0
  fi
  local backoff
  for backoff in 5 20 60; do
    printf '  nested Niri start failed; retrying in %ss (host session may be pausing new clients)\n' \
      "$backoff" >&2
    sleep "$backoff"
    stop_niri
    if start_niri; then
      return 0
    fi
  done
  return 1
}

start_capture_host() {
  if [ "$CAPTURE_NIRI_BACKGROUND" -eq 0 ]; then
    return 0
  fi

  # Niri's nested winit backend is necessarily a host window. Put that
  # backend inside a private virtual KWin compositor so the capture remains
  # a real Wayland/Niri render while no test window can steal the operator's
  # desktop focus or paint over the current work.
  # Keep the socket path below Linux's sockaddr_un limit even when the
  # checkout itself lives under a long mounted workspace path.
  KWIN_RUNTIME="$(mktemp -d /tmp/taskforest-kwin.XXXXXX)"
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
start_niri || ensure_niri || exit 1

# seed_history_fixtures <dir> — deterministic JSONL series for the replay
# panel (roadmap #4): five system series spanning the last ~23h so the 24h
# window shows full waves and the 1h window shows the tail. The exact wire
# format is the history store's contract: {r,c,m,v} per line, one file per
# series named by the HistorySeriesKey stem (system series: `-` device/core).
seed_history_fixtures() {
  timeout 60s python3 - "$1" <<'PY'
import json, os, sys, time

root = sys.argv[1]
now_ms = int(time.time() * 1000)
span_ms = 23 * 3600 * 1000
points = 96

def lines(base, amplitude, period):
    step = span_ms // (points - 1)
    out = []
    for i in range(points):
        value = base + amplitude * ((i % period) / period)
        at = now_ms - span_ms + i * step
        out.append({"r": i + 1, "c": at, "m": at, "v": round(value, 3)})
    return out

fixtures = {
    "cpu-usage-pct__-__-": lines(12.0, 55.0, 17),
    "memory-used-pct__-__-": lines(38.0, 22.0, 31),
    "swap-used-pct__-__-": lines(4.0, 9.0, 23),
    "network-rate-bps__-__-": lines(2_000_000.0, 28_000_000.0, 11),
    "gpu-usage-pct__-__-": lines(8.0, 40.0, 13),
}
for stem, rows in fixtures.items():
    with open(os.path.join(root, stem + ".jsonl"), "w") as handle:
        for row in rows:
            handle.write(json.dumps(row, separators=(",", ":")) + "\n")
PY
}

# seed_application_history_fixtures <dir> — deterministic application CPU,
# memory and process-count series. Every filename is the canonical four-part
# HistorySeriesKey stem, including typed launcher/process provenance. The app
# reads these through the real history-store query and application projection.
seed_application_history_fixtures() {
  timeout 60s python3 - "$1" <<'PY'
import json, os, sys, time

root = sys.argv[1]
now_ms = int(time.time() * 1000)
span_ms = 23 * 3600 * 1000
points = 96

def lines(base, amplitude, period, integral=False):
    step = span_ms // (points - 1)
    out = []
    for i in range(points):
        value = base + amplitude * ((i % period) / period)
        if integral:
            value = round(value)
        at = now_ms - span_ms + i * step
        out.append({"r": i + 1, "c": at, "m": at, "v": round(value, 3)})
    return out

applications = [
    ("launcher:org.mozilla.firefox", 32.0, 1_180_000_000.0, 12.0),
    ("launcher:com.google.Chrome", 24.0, 2_620_000_000.0, 26.0),
    ("launcher:com.visualstudio.code", 18.0, 1_040_000_000.0, 9.0),
    ("launcher:io.github.YellowWhiteBlackCat.TaskForestG", 11.0, 168_000_000.0, 1.0),
    ("process:mihomo", 6.0, 58_000_000.0, 1.0),
]
for index, (identity, cpu, memory, count) in enumerate(applications):
    fixtures = {
        f"application-cpu-usage-pct__-__-__{identity}": lines(cpu, 18.0, 11 + index),
        f"application-memory-bytes__-__-__{identity}": lines(memory, memory * 0.18, 17 + index),
        f"application-process-count__-__-__{identity}": lines(count, 2.0, 19 + index, True),
    }
    for stem, rows in fixtures.items():
        with open(os.path.join(root, stem + ".jsonl"), "w") as handle:
            for row in rows:
                handle.write(json.dumps(row, separators=(",", ":")) + "\n")
PY
}

# capture <name> <skin> <page> <device> <settings> <scenario> <window-size> <capture-size> <attempt>
FAILURES=0
BLOCKED_CAPTURES=0
capture() {
  local name="$1" skin="$2" page="$3" device="$4" settings="$5" scenario="$6" window_size="$7" capture_size="$8" evidence_attempt="$9"
  local attempt_suffix=""
  if [ "$evidence_attempt" -gt 1 ]; then
    attempt_suffix="-attempt-$evidence_attempt"
  fi
  local log="$RUN_DIR/app-$name$attempt_suffix.log"
  local windows_json="$RUN_DIR/window-$name$attempt_suffix.json"
  local windows_json_tmp="$RUN_DIR/window-$name$attempt_suffix.json.tmp"
  local windows_error="$RUN_DIR/window-$name$attempt_suffix.err"
  local action_log="$RUN_DIR/screenshot-$name$attempt_suffix.log"
  local shot="$TMP/$name.png"
  local marker_scenario="${scenario:-standard}"
  local config_home="$NIRI_RUNTIME/config-$name"
  local data_home="$NIRI_RUNTIME/data-$name"
  local expected_width=0 expected_height=0
  IFS=x read -r expected_width expected_height <<<"$capture_size"
  terminate_owned "$APP_PID" "$APP_PGID"
  APP_PID=""
  APP_PGID=""
  rm -f "$shot" "$windows_json" "$windows_json_tmp" "$action_log" 2>/dev/null
  mkdir -p "$config_home"
  # Replay scenarios exercise the REAL persistence path: config opts in and a
  # capture-private XDG data home carries pre-seeded JSONL series. Rows still
  # travel through the history-store query and async application projection.
  local xdg_data_env=()
  case "$scenario" in
    history-replay|application-history-replay)
      mkdir -p "$config_home/taskmanager" "$data_home/taskmanager/history"
      printf '{"history_persistence": true}\n' >"$config_home/taskmanager/config.json"
      if [ "$scenario" = "history-replay" ]; then
        seed_history_fixtures "$data_home/taskmanager/history"
      else
        seed_application_history_fixtures "$data_home/taskmanager/history"
      fi
      printf '  seeded %s history fixture series for %s\n' \
        "$(find "$data_home/taskmanager/history" -name '*.jsonl' 2>/dev/null | wc -l)" "$name"
      xdg_data_env=(XDG_DATA_HOME="$data_home")
      ;;
  esac
  local launch_attempt
  for launch_attempt in 1 2 3; do
    XDG_RUNTIME_DIR="$NIRI_RUNTIME" XDG_CONFIG_HOME="$config_home" WAYLAND_DISPLAY="$SOCK" DISPLAY= \
      LIBGL_ALWAYS_SOFTWARE="$CAPTURE_LIBGL_ALWAYS_SOFTWARE" \
      TM_SKIN="$skin" TM_PAGE="$page" TM_DEVICE="$device" \
      TM_SKIN_HC="" TM_SETTINGS="$settings" \
      TM_CAPTURE_EVIDENCE=1 TM_CAPTURE_SCENARIO="$scenario" \
      TM_WINDOW_SIZE="$window_size" \
      env "${xdg_data_env[@]}" setsid "$APP" &>"$log" & APP_PID=$!
    APP_PGID="$(process_group "$APP_PID")"
    if [ "$APP_PGID" = "$APP_PID" ]; then
      break
    fi
    # A transient spawn failure (the child dying before the group check, or
    # the sandbox refusing the setsid) is worth one immediate retry; only a
    # repeated failure fails the scenario.
    terminate_owned "$APP_PID" "$APP_PGID"
    printf '  retry %s (launch attempt %s did not obtain a private process group)\n' "$name" "$launch_attempt"
    APP_PID="" ; APP_PGID=""
    sleep 1
  done
  if [ "$APP_PGID" != "$APP_PID" ] || [ -z "$APP_PID" ]; then
    printf '  FAIL %s (app did not obtain a private process group; see %s)\n' "$name" "$log"
    FAILURES=$((FAILURES + 1))
    return 1
  fi
  # Poll for the exact current-build window. A broad `grep taskmanager` can
  # match an unrelated window and leaves screenshot-window dependent on focus;
  # bind the Niri window id to this launch's app PID and app_id instead.
  local window_id="" window_ready=missing capture_class=product-or-app
  local i; for i in $(seq 1 25); do
    if NIRI_SOCKET="$IPC" timeout 2s niri msg -j windows >"$windows_json_tmp" 2>"$windows_error"; then
      if jq -e --arg app "$APP_ID" --arg pid "$APP_PID" \
        'any(.[]; .app_id == $app and ((.pid | tostring) == $pid))' \
        "$windows_json_tmp" >/dev/null 2>&1; then
        mv "$windows_json_tmp" "$windows_json"
        window_id="$(jq -r --arg app "$APP_ID" --arg pid "$APP_PID" \
          '.[] | select(.app_id == $app and ((.pid | tostring) == $pid)) | .id' \
          "$windows_json" | head -1)"
        window_ready=ready
        break
      fi
    elif ! NIRI_SOCKET="$IPC" timeout 2s niri msg -j outputs >/dev/null 2>&1; then
      # Only classify a compositor/backend block after a confirming second
      # probe: one expired deadline under host load is not a dead compositor.
      if ! NIRI_SOCKET="$IPC" timeout 2s niri msg -j outputs >/dev/null 2>&1; then
        capture_class=compositor/backend
        break
      fi
    fi
    sleep 1
  done

  # Do not guess readiness with a fixed sleep. Wait until the real app reports
  # that telemetry and background UI data reached RootView; special scenarios
  # must additionally confirm that their controlled state is active.
  local markers=missing
  if [ "$capture_class" != compositor/backend ]; then
    for i in $(seq 1 80); do
      if grep -q "CAPTURE_MARKER event=telemetry_ready scenario=$marker_scenario" "$log" 2>/dev/null \
        && grep -q "CAPTURE_MARKER event=ui_data_ready scenario=$marker_scenario" "$log" 2>/dev/null \
        && grep -q "CAPTURE_MARKER event=theme_ready scenario=$marker_scenario theme=$skin high_contrast=false" "$log" 2>/dev/null \
        && { [ -z "$scenario" ] || grep -q "CAPTURE_MARKER event=scenario_ready scenario=$scenario" "$log" 2>/dev/null; }; then
        markers=ready
        break
      fi
      sleep 0.5
    done
    sleep 1.5 # allow the marker-triggered notify to paint the final frame
  fi

  # A GPUI surface can map after the first window poll even though readiness
  # markers are already present. Re-acquire by the exact PID/app-id before
  # capture; never guess from focus or a broad title match.
  if [ "$markers" = ready ] && [ "$window_ready" != ready ]; then
    for i in $(seq 1 20); do
      if NIRI_SOCKET="$IPC" timeout 2s niri msg -j windows >"$windows_json_tmp" 2>"$windows_error" \
        && jq -e --arg app "$APP_ID" --arg pid "$APP_PID" \
          'any(.[]; .app_id == $app and ((.pid | tostring) == $pid))' \
          "$windows_json_tmp" >/dev/null 2>&1; then
        mv "$windows_json_tmp" "$windows_json"
        window_id="$(jq -r --arg app "$APP_ID" --arg pid "$APP_PID" \
          '.[] | select(.app_id == $app and ((.pid | tostring) == $pid)) | .id' \
          "$windows_json" | head -1)"
        window_ready=ready
        break
      fi
      sleep 0.5
    done
  fi

  # Refresh the ownership receipt after readiness so the explicit id still
  # belongs to the live app immediately before capture.
  if [ "$markers" = ready ] && kill -0 "$APP_PID" 2>/dev/null \
    && NIRI_SOCKET="$IPC" timeout 3s niri msg -j windows >"$windows_json_tmp" 2>"$windows_error" \
    && jq -e --arg app "$APP_ID" --arg pid "$APP_PID" --arg id "$window_id" \
      'any(.[]; .app_id == $app and ((.pid | tostring) == $pid) and ((.id | tostring) == $id))' \
      "$windows_json_tmp" >/dev/null 2>&1; then
    mv "$windows_json_tmp" "$windows_json"
  else
    window_ready=missing
  fi

  # Keep an unavailable compositor distinct from an application failure. The
  # initial output probe succeeded before launch; if the same IPC endpoint is
  # no longer able to answer after the client mapped, this run is blocked by
  # the nested compositor/backend and must not be reported as a product FAIL.
  local niri_health_json="$RUN_DIR/niri-health-$name$attempt_suffix.json"
  local niri_health_error="$RUN_DIR/niri-health-$name$attempt_suffix.err"
  if [ "$capture_class" != compositor/backend ] \
    && ! NIRI_SOCKET="$IPC" timeout 2s niri msg -j outputs \
    >"$niri_health_json" 2>"$niri_health_error"; then
    capture_class=compositor/backend
  fi

  local action=failed
  {
    printf 'app_pid=%s\n' "$APP_PID"
    printf 'window_id=%s\n' "$window_id"
    printf 'capture_class=%s\n' "$capture_class"
    printf 'command=niri msg action screenshot-window --id %s --write-to-disk true --path %s\n' "$window_id" "$shot"
  } >"$action_log"
  if [ "$markers" = ready ] && [ "$window_ready" = ready ] && kill -0 "$APP_PID" 2>/dev/null; then
    NIRI_SOCKET="$IPC" timeout 8s niri msg action screenshot-window \
      --id "$window_id" --write-to-disk true --path "$shot" >>"$action_log" 2>&1 \
      && action=ok
  fi
  local f="$shot"
  if [ "$action" = ok ]; then
    for i in $(seq 1 20); do
      [ -s "$f" ] && break
      sleep 0.1
    done
  fi
  local status=failed width=0 height=0 bytes=0 hash=-
  if [ "$markers" = ready ] && [ "$window_ready" = ready ] && [ "$action" = ok ] \
    && [ -s "$f" ] && [ "$(stat -c%s "$f")" -gt 5000 ]; then
    local dimensions
    dimensions=$(file "$f" | sed -nE 's/.*PNG image data, ([0-9]+) x ([0-9]+).*/\1 \2/p')
    read -r width height <<<"$dimensions"
    if [ -n "$width" ] && [ -n "$height" ] \
      && [ "$width" -eq "$expected_width" ] && [ "$height" -eq "$expected_height" ]; then
      mv "$f" "$STAGED/$name.png"
      bytes=$(stat -c%s "$STAGED/$name.png")
      hash=$(sha256sum "$STAGED/$name.png" | cut -d' ' -f1)
      status=ok
      echo "  ok  $name  (${width}x${height}, $bytes B, markers ready)"
    fi
  fi
  if [ "$status" != ok ]; then
    if [ "$capture_class" = compositor/backend ]; then
      echo "  retry $name (compositor/backend attempt $evidence_attempt: nested Niri IPC stopped responding after client mapping; markers=$markers window=$window_ready id=$window_id action=$action; see $log)"
      rm -f "$f" 2>/dev/null
      terminate_owned "$APP_PID" "$APP_PGID"
      APP_PID=""
      APP_PGID=""
      return 75
    else
      echo "  FAIL $name (product/app: markers=$markers window=$window_ready id=$window_id action=$action expected=${expected_width}x${expected_height}; see $log)"
    fi
    FAILURES=$((FAILURES + 1))
    rm -f "$f" 2>/dev/null
  fi

  local log_hash
  log_hash=$(sha256sum "$log" | cut -d' ' -f1)
  local windows_hash=- action_hash=-
  [ -f "$windows_json" ] && windows_hash=$(sha256sum "$windows_json" | cut -d' ' -f1)
  [ -f "$action_log" ] && action_hash=$(sha256sum "$action_log" | cut -d' ' -f1)
  local log_receipt="target/screenshot-evidence/$RUN_ID/$(basename "$log")"
  local windows_receipt="target/screenshot-evidence/$RUN_ID/$(basename "$windows_json")"
  local action_receipt="target/screenshot-evidence/$RUN_ID/$(basename "$action_log")"
  grep 'CAPTURE_MARKER' "$log" 2>/dev/null | sed "s/^/$name\t/" >>"$MARKERS"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$CAPTURED_AT" "$GIT_HEAD" "$WORKTREE_STATE" "$RUST_VERSION" "$marker_scenario" \
    "$name.png" "$skin" "$page" "$device" "$settings" "$width" "$height" "$bytes" \
    "$hash" "$markers" "$log_receipt" "$log_hash" \
    "target/screenshot-evidence/$RUN_ID/capture-markers.log" >>"$MANIFEST"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$name" "$APP_PID" "$window_id" "$windows_receipt" \
    "$windows_hash" "$action_receipt" "$action_hash" \
    >>"$WINDOW_MANIFEST"

  terminate_owned "$APP_PID" "$APP_PGID"
  APP_PID=""
  APP_PGID=""
  [ "$status" = ok ]
}

echo "capturing evidence matrix -> $RUN_DIR"
EXPECTED_COUNT=0
while IFS=$'\t' read -r name skin page device settings scenario window_size capture_size; do
  [ "$name" = name ] && continue
  [ -z "$name" ] && continue
  if [ "$EXPECTED_COUNT" -gt 0 ] \
    && [ $((EXPECTED_COUNT % CAPTURE_NIRI_BATCH_SIZE)) -eq 0 ]; then
    stop_niri
    ensure_niri || exit 1
  fi
  [ "$scenario" = standard ] && scenario=""
  capture_attempt=1
  while true; do
    capture "$name" "$skin" "$page" "$device" "$settings" "$scenario" \
      "$window_size" "$capture_size" "$capture_attempt"
    capture_status=$?
    [ "$capture_status" -eq 0 ] && break
    if [ "$capture_status" -eq 75 ] \
      && [ "$capture_attempt" -lt "$CAPTURE_NIRI_MAX_ATTEMPTS" ]; then
      stop_niri
      ensure_niri || exit 1
      capture_attempt=$((capture_attempt + 1))
      continue
    fi
    if [ "$capture_status" -eq 75 ]; then
      printf '  BLOCKED %s (compositor/backend remained unavailable after %s attempts)\n' \
        "$name" "$capture_attempt"
      BLOCKED_CAPTURES=$((BLOCKED_CAPTURES + 1))
      FAILURES=$((FAILURES + 1))
    fi
    break
  done
  EXPECTED_COUNT=$((EXPECTED_COUNT + 1))
done <"$MATRIX"

CANONICAL_COUNT="$(awk -F '\t' 'NR > 1 && $1 != "" { count += 1 } END { print count + 0 }' "$CANONICAL_MATRIX")"
if [ "$PUBLISH_CAPTURE" -eq 1 ] && [ "$EXPECTED_COUNT" -ne "$CANONICAL_COUNT" ]; then
  printf 'refusing to publish incomplete matrix: captured=%s canonical=%s; evidence retained at %s\n' \
    "$EXPECTED_COUNT" "$CANONICAL_COUNT" "$RUN_DIR" >&2
  exit 1
fi

if [ "$FAILURES" -ne 0 ]; then
  if [ "$BLOCKED_CAPTURES" -ne 0 ]; then
    printf 'BLOCKED: %s capture(s) unavailable because the nested compositor/backend stopped answering; accepted screenshots were not replaced.\n' "$BLOCKED_CAPTURES"
  fi
  printf 'FAILED: %s capture(s); accepted screenshots were not replaced. Evidence: %s\n' "$FAILURES" "$RUN_DIR"
  exit 1
fi

printf 'niri_instances=%s\n' "$NIRI_START_COUNT" >>"$METADATA"

# The app/runner cannot certify its own evidence. Re-open every PNG, validate
# chunk CRC/dimensions/hash, join the canonical scenario matrix to runtime
# markers, and verify full-log hashes with an independent process.
timeout 30s python3 "$REPO/scripts/validate_capture_evidence.py" \
  --matrix "$MATRIX" \
  --manifest "$MANIFEST" \
  --screenshots "$STAGED" \
  --metadata "$METADATA" \
  --markers "$MARKERS" \
  --niri-outputs "$NIRI_OUTPUTS" \
  --window-receipts "$WINDOW_MANIFEST" \
  --require-binary \
  --repo-root "$REPO" \
  --require-logs \
  --source-manifest "$SOURCE_MANIFEST" \
  --current-worktree \
  --receipt "$VALIDATION" || {
    printf 'FAILED: independent evidence validation rejected the run; accepted screenshots were not replaced. Evidence: %s\n' "$RUN_DIR"
    exit 1
  }

# Promote only a complete matrix to the ignored local latest directory. Every
# image, manifest, and receipt remains local evidence and is never committed.
if [ "$PUBLISH_CAPTURE" -eq 1 ]; then
  for f in "$STAGED"/*.png; do
    install -m0644 "$f" "$OUT/$(basename "$f")"
  done
  install -m0644 "$MANIFEST" "$OUT/capture-manifest.tsv"
  install -m0644 "$METADATA" "$OUT/capture-metadata.txt"
  install -m0644 "$MARKERS" "$OUT/capture-markers.log"
  install -m0644 "$VALIDATION" "$OUT/capture-validation.json"
  install -m0644 "$SOURCE_MANIFEST" "$OUT/gpui-source-manifest.sha256"
  printf '%s\n' "$RUN_ID" >"$EVIDENCE_ROOT/latest.txt"
else
  printf 'targeted capture accepted locally; durable matrix unchanged. Evidence: %s\n' "$RUN_DIR"
fi

if command -v magick >/dev/null 2>&1; then
  magick montage "$STAGED"/*.png -thumbnail 420x280 -tile 3x -geometry +12+18 \
    -background '#202124' "$RUN_DIR/contact-sheet.png"
elif command -v montage >/dev/null 2>&1; then
  montage "$STAGED"/*.png -thumbnail 420x280 -tile 3x -geometry +12+18 \
    -background '#202124' "$RUN_DIR/contact-sheet.png"
fi
if [ -s "$RUN_DIR/contact-sheet.png" ]; then
  printf 'contact sheet kept in run dir only: %s\n' "$RUN_DIR/contact-sheet.png"
fi

echo "DONE: accepted $EXPECTED_COUNT/$EXPECTED_COUNT; durable manifest -> $OUT/capture-manifest.tsv"
echo "Full logs and worktree evidence -> $RUN_DIR"
if [ "$PUBLISH_CAPTURE" -eq 1 ]; then
  ls -la "$OUT"/*.png "$OUT"/capture-manifest.tsv "$OUT"/capture-metadata.txt \
    "$OUT"/capture-markers.log "$OUT"/capture-validation.json 2>/dev/null
fi
