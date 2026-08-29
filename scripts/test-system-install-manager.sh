#!/usr/bin/env bash
# Isolated install/verify/uninstall smoke for the manifest-controlled manager.
# It never targets /usr, /etc, or a user data directory.

set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
SCRATCH_ROOT="$REPO_ROOT/.tmp/agent-runs"
mkdir -p "$SCRATCH_ROOT"
STAGE="$(mktemp -d "$SCRATCH_ROOT/system-install-smoke.XXXXXX")"
CONFLICT_STAGE=""

cleanup() {
    for path in "$STAGE" "$CONFLICT_STAGE"; do
        [[ -n "$path" ]] || continue
        case "$path" in
            "$REPO_ROOT/.tmp/agent-runs/"*) ;;
            *) printf 'error: refusing to clean unexpected smoke path: %s\n' "$path" >&2; exit 1 ;;
        esac
        [[ -d "$path" && ! -L "$path" ]] || exit 1
        rm -rf -- "$path"
    done
}

trap cleanup EXIT
trap 'exit 130' INT TERM

mkdir -p "$STAGE/usr/lib" "$STAGE/usr/libexec" "$STAGE/usr/share/polkit-1/actions"
for feature in perf net process; do
    case "$feature" in
        perf)
            helper="/usr/libexec/taskmanager-privilege-helper"
            policy="/usr/share/polkit-1/actions/com.taskforest.perf-helper.policy"
            ;;
        net)
            helper="/usr/libexec/taskmanager-net-launcher"
            policy="/usr/share/polkit-1/actions/com.taskforest.net-launcher.policy"
            ;;
        process)
            helper="/usr/lib/taskforest-process-control-helper"
            policy="/usr/share/polkit-1/actions/com.taskforest.process-control.policy"
            ;;
    esac
    CONFLICT_STAGE="$(mktemp -d "$SCRATCH_ROOT/system-install-conflict.XXXXXX")"
    mkdir -p "$CONFLICT_STAGE/usr/lib" "$CONFLICT_STAGE/usr/libexec" \
        "$CONFLICT_STAGE/usr/share/polkit-1/actions"
    install -m 0644 /dev/null "$CONFLICT_STAGE$helper"
    if timeout --kill-after=10s 30s "$REPO_ROOT/scripts/manage-polkit-install.sh" \
        install "$feature" --staging "$CONFLICT_STAGE"; then
        printf 'error: %s conflict smoke overwrote a different file\n' "$feature" >&2
        exit 1
    fi
    [[ -f "$CONFLICT_STAGE$helper" && ! -s "$CONFLICT_STAGE$helper" ]]
    rm -rf -- "$CONFLICT_STAGE"
    CONFLICT_STAGE=""

    timeout --kill-after=10s 30s "$REPO_ROOT/scripts/manage-polkit-install.sh" \
        install "$feature" --staging "$STAGE"
    timeout --kill-after=10s 30s "$REPO_ROOT/scripts/manage-polkit-install.sh" \
        verify "$feature" --staging "$STAGE"
    timeout --kill-after=10s 30s "$REPO_ROOT/scripts/manage-polkit-install.sh" \
        uninstall "$feature" --staging "$STAGE"
    [[ ! -e "$STAGE$helper" && ! -e "$STAGE$policy" ]]
done

DEV_ROOT="$STAGE/developer-user"
DEV_XDG="$DEV_ROOT/xdg"
DEV_HOME="$DEV_ROOT/home"
DEV_BIN="$DEV_ROOT/bin"
MOCK_BIN="$DEV_ROOT/mock-bin"
HICOLOR_FIXTURE="$DEV_ROOT/index.theme"
CACHE_LOG="$DEV_ROOT/cache.log"
mkdir -p "$DEV_XDG/applications" "$DEV_HOME" "$DEV_BIN" "$MOCK_BIN"
install -m 0755 /bin/true "$DEV_BIN/taskforest-g"
install -m 0755 /bin/true "$DEV_BIN/taskforest-i"
printf '[Icon Theme]\nName=Smoke hicolor\nDirectories=scalable/apps\n' >"$HICOLOR_FIXTURE"

cat >"$MOCK_BIN/cache-command" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\t%s\n' "$(basename "$0")" "$*" >>"${TASKFOREST_CACHE_LOG:?}"
EOF
chmod 0755 "$MOCK_BIN/cache-command"
for command in update-desktop-database gtk-update-icon-cache kbuildsycoca6; do
    ln -s cache-command "$MOCK_BIN/$command"
done

run_frontend_manager() {
    env XDG_DATA_HOME="$DEV_XDG" XDG_CONFIG_HOME="$DEV_HOME/.config" HOME="$DEV_HOME" \
        TASKFOREST_HICOLOR_INDEX_SOURCE="$HICOLOR_FIXTURE" \
        TASKFOREST_CACHE_LOG="$CACHE_LOG" PATH="$MOCK_BIN:$PATH" \
        timeout --kill-after=10s 30s "$REPO_ROOT/scripts/dev-install-frontends.sh" "$@"
}

run_frontend_manager_with_data_home() {
    local data_home="$1"
    shift
    env XDG_DATA_HOME="$data_home" XDG_CONFIG_HOME="$DEV_HOME/.config" HOME="$DEV_HOME" \
        TASKFOREST_HICOLOR_INDEX_SOURCE="$HICOLOR_FIXTURE" \
        TASKFOREST_CACHE_LOG="$CACHE_LOG" PATH="$MOCK_BIN:$PATH" \
        timeout --kill-after=10s 30s "$REPO_ROOT/scripts/dev-install-frontends.sh" "$@"
}

GPUI_DESKTOP="$DEV_XDG/applications/io.github.YellowWhiteBlackCat.TaskForestG.desktop"
ICED_DESKTOP="$DEV_XDG/applications/io.github.YellowWhiteBlackCat.TaskForestI.desktop"
SHARED_ICON="$DEV_XDG/icons/hicolor/scalable/apps/taskforest-taskboard.svg"
HICOLOR_INDEX="$DEV_XDG/icons/hicolor/index.theme"
OWNERSHIP_RECEIPT="$DEV_XDG/taskforest/dev-install-frontends.tsv"

run_frontend_manager "$DEV_BIN/taskforest-g" "$DEV_BIN/taskforest-i"
run_frontend_manager "$DEV_BIN/taskforest-g" "$DEV_BIN/taskforest-i"
[[ -f "$GPUI_DESKTOP" && -f "$ICED_DESKTOP" && -f "$SHARED_ICON" ]]
[[ -f "$HICOLOR_INDEX" && -f "$OWNERSHIP_RECEIPT" ]]
# NTFS mounts used by Git Bash/MSYS do not preserve POSIX mode bits. Keep the
# strict receipt-permission assertion on real POSIX filesystems, where it is a
# security contract rather than a portable filesystem property.
case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) ;;
    *) [[ "$(stat -c %a "$OWNERSHIP_RECEIPT")" == "600" ]] ;;
esac
grep -Fx "Exec=env MALLOC_ARENA_MAX=1 $(realpath -e "$DEV_BIN/taskforest-g")" "$GPUI_DESKTOP" >/dev/null
grep -Fx "Exec=env MALLOC_ARENA_MAX=1 $(realpath -e "$DEV_BIN/taskforest-i")" "$ICED_DESKTOP" >/dev/null
cmp -s "$HICOLOR_FIXTURE" "$HICOLOR_INDEX"
grep -q '^kbuildsycoca6' "$CACHE_LOG"

# A different data root has no receipt and must be a harmless no-op; it must
# not gain authority over the integration installed in DEV_XDG.
run_frontend_manager_with_data_home "$DEV_ROOT/different-data" --uninstall
[[ -f "$GPUI_DESKTOP" && -f "$ICED_DESKTOP" && -f "$SHARED_ICON" && -f "$OWNERSHIP_RECEIPT" ]]

install -m 0644 /dev/null "$SHARED_ICON"
if run_frontend_manager --uninstall; then
    printf 'error: developer uninstall ignored a changed managed icon\n' >&2
    exit 1
fi
[[ -f "$GPUI_DESKTOP" && -f "$ICED_DESKTOP" && -f "$OWNERSHIP_RECEIPT" ]]
install -m 0644 "$REPO_ROOT/packaging/linux/io.github.YellowWhiteBlackCat.TaskForest.svg" \
    "$SHARED_ICON"
run_frontend_manager --uninstall
[[ ! -e "$GPUI_DESKTOP" && ! -e "$ICED_DESKTOP" && ! -e "$SHARED_ICON" ]]
[[ ! -e "$HICOLOR_INDEX" && ! -e "$OWNERSHIP_RECEIPT" ]]

# A schema-1 receipt from before the current receipt format remains a valid
# removal authority for its exact desktop/icon set.
run_frontend_manager "$DEV_BIN/taskforest-g" "$DEV_BIN/taskforest-i"
awk -F '\t' 'BEGIN { OFS="\t" }
    $1 == "schema" { print "schema", "1"; next }
    { print }
' "$OWNERSHIP_RECEIPT" >"$DEV_ROOT/legacy-receipt.tsv"
mv -- "$DEV_ROOT/legacy-receipt.tsv" "$OWNERSHIP_RECEIPT"
run_frontend_manager --uninstall
[[ ! -e "$GPUI_DESKTOP" && ! -e "$ICED_DESKTOP" && ! -e "$SHARED_ICON" ]]
[[ ! -e "$HICOLOR_INDEX" && ! -e "$OWNERSHIP_RECEIPT" ]]

EXTERNAL_ROOT="$STAGE/developer-external-index"
DEV_XDG="$EXTERNAL_ROOT/xdg"
DEV_HOME="$EXTERNAL_ROOT/home"
CACHE_LOG="$EXTERNAL_ROOT/cache.log"
mkdir -p "$DEV_XDG/icons/hicolor" "$DEV_HOME"
printf '[Icon Theme]\nName=Externally owned\n' >"$DEV_XDG/icons/hicolor/index.theme"
cp "$DEV_XDG/icons/hicolor/index.theme" "$EXTERNAL_ROOT/index.before"
run_frontend_manager "$DEV_BIN/taskforest-g" "$DEV_BIN/taskforest-i"
cmp -s "$EXTERNAL_ROOT/index.before" "$DEV_XDG/icons/hicolor/index.theme"
run_frontend_manager --uninstall
cmp -s "$EXTERNAL_ROOT/index.before" "$DEV_XDG/icons/hicolor/index.theme"

CONFLICT_ROOT="$STAGE/developer-conflict"
DEV_XDG="$CONFLICT_ROOT/xdg"
DEV_HOME="$CONFLICT_ROOT/home"
CACHE_LOG="$CONFLICT_ROOT/cache.log"
mkdir -p "$DEV_XDG/icons/hicolor/scalable/apps" "$DEV_HOME"
printf 'different user icon\n' >"$DEV_XDG/icons/hicolor/scalable/apps/taskforest-taskboard.svg"
cp "$DEV_XDG/icons/hicolor/scalable/apps/taskforest-taskboard.svg" "$CONFLICT_ROOT/icon.before"
if run_frontend_manager "$DEV_BIN/taskforest-g" "$DEV_BIN/taskforest-i"; then
    printf 'error: developer install overwrote a conflicting user file\n' >&2
    exit 1
fi
cmp -s "$CONFLICT_ROOT/icon.before" \
    "$DEV_XDG/icons/hicolor/scalable/apps/taskforest-taskboard.svg"
[[ ! -e "$DEV_XDG/applications/io.github.YellowWhiteBlackCat.TaskForestG.desktop" ]]
[[ ! -e "$DEV_XDG/applications/io.github.YellowWhiteBlackCat.TaskForestI.desktop" ]]
[[ ! -e "$DEV_XDG/taskforest/dev-install-frontends.tsv" ]]

printf 'system-install-manager isolated smoke: PASS\n'
