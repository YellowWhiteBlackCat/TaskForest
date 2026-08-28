#!/usr/bin/env bash
# Capture the current taskforest-b build in a private nested Wayland/Niri
# compositor. The app id, PID and Niri window id are matched exactly;
# focused-window or stale PNG capture is not accepted. By default Niri is
# hosted by a private virtual KWin framebuffer, so the operator's desktop is
# not disturbed; set TM_CAPTURE_NIRI_BACKGROUND=0 for visible debugging.
set -euo pipefail
export LC_ALL=C

REPO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO"
MATRIX="$REPO/scripts/capture_bevy_scenarios.tsv"
APP_ID="io.github.YellowWhiteBlackCat.TaskForestB"
RUN_ID="$(date -u +%Y%m%dT%H%M%SZ)-$$"
RUN_DIR="$REPO/target/bevy-evidence/$RUN_ID"
APP="$RUN_DIR/taskforest-b"
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
    0|1) ;;
    *) die 'TM_CAPTURE_NIRI_BACKGROUND must be 0 or 1' ;;
esac
if [ "$CAPTURE_NIRI_BACKGROUND" -eq 1 ]; then
    need kwin_wayland
fi
HOST_XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR"
HOST_WAYLAND_DISPLAY="$WAYLAND_DISPLAY"

git status --short >"$RUN_DIR/git-status.txt"
git diff --binary HEAD >"$RUN_DIR/worktree.diff"
git rev-parse HEAD >"$RUN_DIR/git-head.txt"
rustc -V >"$RUN_DIR/rust.txt"
PYTHONDONTWRITEBYTECODE=1 timeout 60s python3 scripts/frontend_source_manifest.py \
    --frontend bevy --repo-root "$REPO" --output "$SOURCE_MANIFEST"
timeout --kill-after=10s 20m cargo build --locked -p taskmanager-bevy-ui --bin taskforest-b
install -Dm755 "$REPO/target/debug/taskforest-b" "$APP"
BINARY_SHA256="$(sha256sum "$APP" | cut -d' ' -f1)"
SOURCE_MANIFEST_SHA256="$(sha256sum "$SOURCE_MANIFEST" | cut -d' ' -f1)"

# Keep Niri's IPC and Wayland socket paths below Linux's sockaddr_un limit even
# when the checkout itself lives under a long mounted workspace path.
RUNTIME_DIR="$(mktemp -d /tmp/taskforest-bevy.XXXXXX)"
NIRI_PID=""
NIRI_PGID=""
KWIN_PID=""
KWIN_PGID=""
KWIN_RUNTIME=""
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
    if [ -n "$KWIN_RUNTIME" ] && [ -d "$KWIN_RUNTIME" ]; then
        rm -rf -- "$KWIN_RUNTIME"
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
    KWIN_RUNTIME="$(mktemp -d /tmp/taskforest-bevy-kwin.XXXXXX)"
    KWIN_DISPLAY="$KWIN_RUNTIME/wayland-outer"
    mkdir -p "$KWIN_RUNTIME"
    chmod 700 "$KWIN_RUNTIME"
    XDG_RUNTIME_DIR="$KWIN_RUNTIME" WAYLAND_DISPLAY= DISPLAY= \
        QT_QPA_PLATFORM=wayland setsid timeout --foreground --kill-after=10s 20m \
        kwin_wayland --virtual --socket=wayland-outer --width=1920 --height=1080 \
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

XDG_RUNTIME_DIR="$RUNTIME_DIR" WAYLAND_DISPLAY="$NIRI_PARENT_WAYLAND" \
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
    XDG_RUNTIME_DIR="$RUNTIME_DIR" WAYLAND_DISPLAY="$SOCK" \
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
    printf 'captured_at=%s\n' "$CAPTURED_AT"
    printf 'git_head=%s\n' "$(cat "$RUN_DIR/git-head.txt")"
    printf 'worktree_sha256=%s\n' "$(sha256sum "$RUN_DIR/git-status.txt" | cut -d' ' -f1)"
    printf 'rust=%s\n' "$(cat "$RUN_DIR/rust.txt")"
    printf 'binary=taskforest-b --demo\n'
    printf 'binary_sha256=%s\n' "$BINARY_SHA256"
    printf 'app_id=%s\n' "$APP_ID"
    printf 'capture_backend=niri-screenshot-window-wayland\n'
    if [ "$CAPTURE_NIRI_BACKGROUND" -eq 1 ]; then
        printf 'niri_host=kwin-wayland-virtual\n'
    else
        printf 'niri_host=host-wayland-visible\n'
    fi
    printf 'niri_background=%s\n' "$CAPTURE_NIRI_BACKGROUND"
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
