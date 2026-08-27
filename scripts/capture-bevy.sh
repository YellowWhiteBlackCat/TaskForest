#!/usr/bin/env bash
# Capture the current taskforest-b build in a private nested Wayland/Niri
# compositor. The app id, PID and Niri window id are matched exactly;
# focused-window or stale PNG capture is not accepted.
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

for command in cargo niri jq file setsid timeout sha256sum; do need "$command"; done
[ -n "${WAYLAND_DISPLAY:-}" ] || die 'WAYLAND_DISPLAY is not set'
[ -n "${XDG_RUNTIME_DIR:-}" ] || die 'XDG_RUNTIME_DIR is not set'
[ -s "$MATRIX" ] || die "missing capture matrix: $MATRIX"
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

RUNTIME_DIR="$(mktemp -d "$REPO/.tmp/bevy-wayland.XXXXXX")"
NIRI_PID=""
cleanup() {
    if [ -n "$NIRI_PID" ] && kill -0 "$NIRI_PID" 2>/dev/null; then
        kill "$NIRI_PID" 2>/dev/null || true
        wait "$NIRI_PID" 2>/dev/null || true
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
# The nested compositor is itself a Wayland client of the real host compositor.
# Keep the host runtime for this connection; only child applications switch to
# the socket that nested Niri creates below.
XDG_RUNTIME_DIR="$HOST_XDG_RUNTIME_DIR" WAYLAND_DISPLAY="$HOST_WAYLAND_DISPLAY" \
    LIBGL_ALWAYS_SOFTWARE=1 setsid timeout --foreground --kill-after=10s 20m \
    niri --config "$CONF" >"$RUN_DIR/niri.log" 2>&1 &
NIRI_PID=$!

SOCK=""; IPC=""
for _ in $(seq 1 80); do
    SOCK="$(grep -oE 'wayland-[0-9]+' "$RUN_DIR/niri.log" | head -1 || true)"
    IPC="$(grep -oE "$HOST_XDG_RUNTIME_DIR/niri\.[^ ]*\.sock" "$RUN_DIR/niri.log" | head -1 || true)"
    if [ -n "$SOCK" ] && [ -S "$HOST_XDG_RUNTIME_DIR/$SOCK" ] && [ -n "$IPC" ]; then break; fi
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
    XDG_RUNTIME_DIR="$HOST_XDG_RUNTIME_DIR" WAYLAND_DISPLAY="$SOCK" \
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
