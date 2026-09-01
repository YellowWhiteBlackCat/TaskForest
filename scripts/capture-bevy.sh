#!/usr/bin/env bash
# Capture the current taskforest-b build in a private nested Wayland/Niri
# compositor. The app id, PID and Niri window id are matched exactly;
# focused-window or stale PNG capture is not accepted. By default Niri is
# hosted by a private virtual KWin framebuffer, so the operator's desktop is
# not disturbed; visible compositor mode is rejected by the acceptance path.
set -euo pipefail
export LC_ALL=C

REPO="$(cd "$(dirname "$0")/.." && pwd)"
if [ "${TM_CAPTURE_SUPERVISED:-0}" != "1" ] \
    || [ "${TM_CAPTURE_SUPERVISOR_TOKEN:-}" != "${TM_CAPTURE_RUN_UUID:-}" ]; then
    command -v python3 >/dev/null 2>&1 \
        || { printf 'capture requires the supervisor interpreter\n' >&2; exit 2; }
    command -v timeout >/dev/null 2>&1 \
        || { printf 'capture requires timeout for supervisor lifetime bounding\n' >&2; exit 2; }
    exec timeout --kill-after=10s 30m python3 "$REPO/scripts/capture_supervisor.py" \
        --repo-root "$REPO" --frontend bevy -- bash "$0" "$@"
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
        || { printf 'background Bevy capture requires dbus-run-session\n' >&2; exit 2; }
    private_session_config="$(cd "$(dirname "$0")" && pwd)/private-session.conf"
    TM_CAPTURE_PRIVATE_DBUS=1 exec dbus-run-session \
        --config-file="$private_session_config" -- bash "$0" "$@"
fi

cd "$REPO"
MATRIX="$REPO/scripts/capture_bevy_scenarios.tsv"
APP_ID="io.github.YellowWhiteBlackCat.TaskForestB"
RUN_ID="$CAPTURE_RUN_UUID"
RUN_DIR="$CAPTURE_RUN_ROOT"
RUN_RELATIVE="${RUN_DIR#"$REPO/"}"
APP="$RUN_DIR/bin/taskforest-b"
SOURCE_MANIFEST="$RUN_DIR/source-manifest.sha256"
CONF="$RUN_DIR/niri.kdl"
mkdir -p "$RUN_DIR"

die() { printf 'Bevy Wayland capture: FAIL: %s\n' "$1" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "required command unavailable: $1"; }

for command in cargo niri jq file setsid timeout sha256sum ps; do need "$command"; done
[ -n "${WAYLAND_DISPLAY:-}" ] || die 'WAYLAND_DISPLAY is not set'
[ -n "${XDG_RUNTIME_DIR:-}" ] || die 'XDG_RUNTIME_DIR is not set'
[ -s "$MATRIX" ] || die "missing capture matrix: $MATRIX"
CAPTURE_NIRI_BACKGROUND="${TM_CAPTURE_NIRI_BACKGROUND:-1}"
case "$CAPTURE_NIRI_BACKGROUND" in
    1) ;;
    0) die 'visible Bevy capture is disabled; use the private background route' ;;
    *) die 'TM_CAPTURE_NIRI_BACKGROUND must be 1' ;;
esac
if [ "$CAPTURE_NIRI_BACKGROUND" -eq 1 ]; then
    need kwin_wayland
fi
if [ "$CAPTURE_NIRI_BACKGROUND" -eq 1 ]; then
    case "${DBUS_SESSION_BUS_ADDRESS:-}" in
        unix:path=/tmp/dbus-*,guid=*) ;;
        *) die 'background Bevy capture requires the private-session D-Bus address' ;;
    esac
fi
DBUS_ADDRESS_SHA256="$(printf '%s' "${DBUS_SESSION_BUS_ADDRESS:-}" | sha256sum | cut -d' ' -f1)"
case "$DBUS_ADDRESS_SHA256" in
    [0-9a-f][0-9a-f][0-9a-f][0-9a-f]*) ;;
    *) die 'private D-Bus address hash could not be recorded' ;;
esac
HOST_XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR"
HOST_WAYLAND_DISPLAY="$WAYLAND_DISPLAY"

git status --short >"$RUN_DIR/git-status.txt"
git diff --binary HEAD >"$RUN_DIR/worktree.diff"
git rev-parse HEAD >"$RUN_DIR/git-head.txt"
rustc -V >"$RUN_DIR/rust.txt"
PYTHONDONTWRITEBYTECODE=1 timeout 60s python3 scripts/frontend_source_manifest.py \
    --frontend bevy --repo-root "$REPO" --output "$SOURCE_MANIFEST"
# Lock policy follows the caller (dev-phase fallback: TM_CARGO_LOCK empty
# runs unlocked while a sibling line holds the shared lock mid-write).
LOCK_ARGS=(--locked)
if [[ -n "${TM_CARGO_LOCK+x}" && -z "$TM_CARGO_LOCK" ]]; then
    LOCK_ARGS=()
fi
timeout --kill-after=10s 20m python3 scripts/capture_build.py \
    --repo-root "$REPO" --source "$REPO/target/debug/taskforest-b" \
    --destination "$APP" -- cargo build "${LOCK_ARGS[@]}" \
    -p taskmanager-bevy-ui --bin taskforest-b
BINARY_SHA256="$(sha256sum "$APP" | cut -d' ' -f1)"
SOURCE_MANIFEST_SHA256="$(sha256sum "$SOURCE_MANIFEST" | cut -d' ' -f1)"

# Keep Niri's IPC and Wayland socket paths below Linux's sockaddr_un limit even
# when the checkout itself lives under a long mounted workspace path.
RUNTIME_DIR="$CAPTURE_RUNTIME_ROOT/niri"
mkdir -p "$RUNTIME_DIR" "$RUNTIME_DIR/config" "$RUNTIME_DIR/data" \
    "$RUNTIME_DIR/cache" "$RUNTIME_DIR/state" "$RUN_DIR/bin"
chmod 700 "$RUNTIME_DIR"
NIRI_PID=""
NIRI_PGID=""
KWIN_PID=""
KWIN_PGID=""
KWIN_RUNTIME=""
KWIN_ROOT=""
KWIN_SOCKET=""
KWIN_DISPLAY=""
NIRI_PARENT_WAYLAND="$HOST_WAYLAND_DISPLAY"

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
    if [ -n "$NIRI_PID" ] && kill -0 "$NIRI_PID" 2>/dev/null; then
        kill "$NIRI_PID" 2>/dev/null || true
        wait "$NIRI_PID" 2>/dev/null || true
    fi
    terminate_owned "$KWIN_PID" "$KWIN_PGID"
    if [ -n "$KWIN_ROOT" ] && [ -d "$KWIN_ROOT" ]; then
        case "$KWIN_ROOT" in
            "$CAPTURE_RUNTIME_ROOT/kwin") rm -rf -- "$KWIN_ROOT" ;;
            *) printf 'cleanup refused unexpected KWin root: %s\n' "$KWIN_ROOT" >&2 ;;
        esac
    fi
    rm -rf "$RUNTIME_DIR"
}
trap cleanup EXIT

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
    # Keep the socket path below Linux's sockaddr_un limit even when the
    # checkout itself lives under a long mounted workspace path.
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
    [ "$KWIN_PGID" = "$KWIN_PID" ] || die 'virtual KWin did not obtain a private process group'
    for _ in $(seq 1 40); do
        [ -S "$KWIN_DISPLAY" ] && break
        sleep 0.2
    done
    if ! kill -0 "$KWIN_PID" 2>/dev/null || [ ! -S "$KWIN_DISPLAY" ]; then
        tail -20 "$RUN_DIR/kwin-wayland.log" >&2 || true
        die 'virtual KWin did not start'
    fi
    NIRI_PARENT_WAYLAND="$KWIN_DISPLAY"
    printf 'background capture host: kwin-wayland --virtual (%s)\n' "$KWIN_DISPLAY"
}

start_capture_host

XDG_RUNTIME_DIR="$RUNTIME_DIR" XDG_CONFIG_HOME="$RUNTIME_DIR/config" \
    XDG_DATA_HOME="$RUNTIME_DIR/data" XDG_CACHE_HOME="$RUNTIME_DIR/cache" \
    XDG_STATE_HOME="$RUNTIME_DIR/state" WAYLAND_DISPLAY="$NIRI_PARENT_WAYLAND" \
    LIBGL_ALWAYS_SOFTWARE=1 RUST_LOG=niri=info setsid timeout --foreground --kill-after=10s 20m \
    niri --config "$CONF" >"$RUN_DIR/niri.log" 2>&1 &
NIRI_PID=$!
NIRI_PGID="$(process_group "$NIRI_PID")"

SOCK=""; IPC=""
for _ in $(seq 1 80); do
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
    if [ -n "$SOCK" ] && [ -S "$RUNTIME_DIR/$SOCK" ] && [ -n "$IPC" ]; then break; fi
    sleep 0.2
done
if grep -q 'NoCompositor' "$RUN_DIR/niri.log" 2>/dev/null; then
    printf 'Bevy Wayland capture: SKIP: host Wayland socket has no compositor\n'
    printf '  evidence=%s/niri.log\n' "$RUN_DIR"
    exit 2
fi
[ -n "$SOCK" ] && [ -n "$IPC" ] || die 'nested Niri did not start'
NIRI_SOCKET="$IPC" timeout 10s niri msg -j outputs >"$RUN_DIR/niri-outputs.json"

FILTERED="$RUN_DIR/matrix.tsv"
if [ -n "${TM_BEVY_CAPTURE_SCENARIOS:-}" ]; then
    awk -F '\t' -v wanted="$TM_BEVY_CAPTURE_SCENARIOS" \
        'BEGIN { split(wanted, a, ","); for (i in a) ok[a[i]]=1 } NR == 1 || ok[$1]' \
        "$MATRIX" >"$FILTERED"
else
    cp "$MATRIX" "$FILTERED"
fi
COUNT="$(awk -F '\t' 'NR > 1 && $1 != "" { n++ } END { print n + 0 }' "$FILTERED")"
[ "$COUNT" -gt 0 ] || die 'capture matrix selection is empty'

printf 'scenario\tpage\trequested_window\timage\tmarkers\twindows\taction\tapp_pid\twindow_id\twidth\theight\tbytes\tsha256\tstatus\n' >"$RUN_DIR/manifest.tsv"

capture_one() {
    local name="$1" page="$2" window_size="$3"
    local scenario_dir="$RUN_DIR/$name" log="$RUN_DIR/$name/app.log"
    local markers="$scenario_dir/markers.log" windows="$scenario_dir/windows.json"
    local action="$scenario_dir/action.log" image="$scenario_dir/image.png"
    mkdir -p "$scenario_dir"
    local width=0 height=0 bytes=0 hash=- status=failed app_pid="" window_id=""
    local expected_width expected_height
    IFS=x read -r expected_width expected_height <<<"$window_size"
    XDG_RUNTIME_DIR="$RUNTIME_DIR" XDG_CONFIG_HOME="$RUNTIME_DIR/config" \
        XDG_DATA_HOME="$RUNTIME_DIR/data" XDG_CACHE_HOME="$RUNTIME_DIR/cache" \
        XDG_STATE_HOME="$RUNTIME_DIR/state" WAYLAND_DISPLAY="$SOCK" \
        TM_BEVY_CAPTURE_PAGE="$page" TM_BEVY_WINDOW_SIZE="$window_size" \
        LIBGL_ALWAYS_SOFTWARE=1 setsid "$APP" --demo >"$log" 2>&1 &
    app_pid=$!
    for _ in $(seq 1 120); do
        if grep -q "BEVY_CAPTURE_MARKER event=frame_ready mode=demo page=$page" "$log" 2>/dev/null \
            && grep -q "BEVY_CAPTURE_MARKER event=target_ready mode=demo page=$page" "$log" 2>/dev/null; then
            break
        fi
        kill -0 "$app_pid" 2>/dev/null || break
        sleep 0.1
    done
    grep 'BEVY_CAPTURE_MARKER' "$log" >"$markers" 2>/dev/null || true
    sleep "${TM_BEVY_CAPTURE_SETTLE_SECONDS:-0.5}"
    NIRI_SOCKET="$IPC" timeout 5s niri msg -j windows >"$windows" 2>/dev/null || true
    window_id="$(jq -r --arg app "$APP_ID" --arg pid "$app_pid" \
        '[.[] | select(.app_id == $app and ((.pid|tostring) == $pid))] | if length == 1 then .[0].id else empty end' \
        "$windows" 2>/dev/null || true)"
    if [ -n "$window_id" ]; then
        printf 'window_id=%s\n' "$window_id" >"$action"
        printf 'action=screenshot-window --id %s --write-to-disk true --path %s\n' \
            "$window_id" "$image" >>"$action"
        if NIRI_SOCKET="$IPC" timeout 8s niri msg action screenshot-window \
            --id "$window_id" --write-to-disk true --path "$image" >>"$action" 2>&1; then
            # Niri acknowledges the action before the PNG writer has flushed
            # the file. Wait for a parseable PNG, otherwise the receipt can
            # race the writer and record a false 0x0 image.
            for _ in $(seq 1 50); do
                if [ -s "$image" ] && file "$image" | grep -q 'PNG image data'; then
                    break
                fi
                sleep 0.1
            done
            if [ -s "$image" ]; then
                read -r width height < <(file "$image" | sed -nE 's/.*PNG image data, ([0-9]+) x ([0-9]+).*/\1 \2/p')
                bytes="$(stat -c%s "$image" 2>/dev/null || echo 0)"
                hash="$(sha256sum "$image" | cut -d' ' -f1)"
                if [ "${width:-0}" -ge "$expected_width" ] && [ "${height:-0}" -ge "$expected_height" ]; then
                    status=ok
                fi
            fi
        fi
    fi
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$name" "$page" "$window_size" "$name/image.png" "$name/markers.log" \
        "$name/windows.json" "$name/action.log" "$app_pid" "$window_id" \
        "$width" "$height" "$bytes" "$hash" "$status" >>"$RUN_DIR/manifest.tsv"
    kill "$app_pid" 2>/dev/null || true
    wait "$app_pid" 2>/dev/null || true
    [ "$status" = ok ]
}

failures=0
while IFS=$'\t' read -r name page window_size; do
    [ "$name" = name ] && continue
    [ -n "$name" ] || continue
    if capture_one "$name" "$page" "$window_size"; then
        printf '  PASS %-22s\n' "$name"
    else
        printf '  FAIL %-22s (see %s)\n' "$name" "$RUN_DIR" >&2
        failures=$((failures + 1))
    fi
done <"$FILTERED"
[ "$failures" -eq 0 ] || die "$failures scenario(s) failed; evidence retained at $RUN_DIR"

CAPTURED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
{
    printf 'run_id=%s\n' "$RUN_ID"
    printf 'run_uuid=%s\n' "$CAPTURE_RUN_UUID"
    printf 'frontend=bevy\n'
    printf 'run_root=%s\n' "$RUN_RELATIVE"
    printf 'runtime_root=%s\n' "$CAPTURE_RUNTIME_ROOT"
    printf 'supervisor_pid=%s\n' "${TM_CAPTURE_SUPERVISOR_PID:-}"
    printf 'cgroup_path=%s\n' "${TM_CAPTURE_CGROUP_PATH:-}"
    printf 'captured_at=%s\n' "$CAPTURED_AT"
    printf 'git_head=%s\n' "$(cat "$RUN_DIR/git-head.txt")"
    printf 'worktree_sha256=%s\n' "$(sha256sum "$RUN_DIR/git-status.txt" | cut -d' ' -f1)"
    printf 'rust=%s\n' "$(cat "$RUN_DIR/rust.txt")"
    printf 'binary=%s\n' "${APP#"$REPO/"}"
    printf 'binary_sha256=%s\n' "$BINARY_SHA256"
    printf 'app_id=%s\n' "$APP_ID"
    printf 'capture_backend=niri-screenshot-window-wayland\n'
    if [ "$CAPTURE_NIRI_BACKGROUND" -eq 1 ]; then
        printf 'niri_host=kwin-wayland-virtual\n'
    else
        printf 'niri_host=host-wayland-visible\n'
    fi
    printf 'niri_background=%s\n' "$CAPTURE_NIRI_BACKGROUND"
    printf 'dbus_isolation=private-session\n'
    printf 'dbus_address_sha256=%s\n' "$DBUS_ADDRESS_SHA256"
    printf 'matrix=scripts/capture_bevy_scenarios.tsv\n'
    printf 'scenario_count=%s\n' "$COUNT"
    printf 'source_scope=bevy\n'
    printf 'source_manifest_sha256=%s\n' "$SOURCE_MANIFEST_SHA256"
    printf 'command=bash scripts/capture-bevy.sh\n'
} >"$RUN_DIR/metadata.txt"
PYTHONDONTWRITEBYTECODE=1 timeout 30s python3 scripts/validate_bevy_matrix.py \
    --matrix "$FILTERED" --manifest "$RUN_DIR/manifest.tsv" --run-dir "$RUN_DIR" \
    --metadata "$RUN_DIR/metadata.txt" --source-manifest "$SOURCE_MANIFEST" \
    --niri-outputs "$RUN_DIR/niri-outputs.json" --receipt "$RUN_DIR/bevy-capture-validation.json" \
    --repo-root "$REPO" --binary "$APP" --current-worktree
printf 'Bevy Wayland capture: PASS -> %s\n' "$RUN_DIR"
