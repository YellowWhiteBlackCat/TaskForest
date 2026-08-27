#!/usr/bin/env bash
# Install both frontend shapes as separate user-local desktop applications.
#
# Build artifacts with scripts/build-frontend-binaries.sh first, or pass the
# two executable paths explicitly. The managed desktop IDs and Exec paths are
# distinct, so GPUI and Iced can remain installed and launched together.
#
# The ownership receipt is the removal authority. Existing files are adopted
# only when their content is exactly what this invocation would install; this
# script never overwrites a different desktop entry, icon, or icon-theme index.
#
# Usage:
#   scripts/dev-install-frontends.sh
#   scripts/dev-install-frontends.sh path/to/taskforest-g path/to/taskforest-i
#   scripts/dev-install-frontends.sh --uninstall
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
APPS_DIR="$DATA_DIR/applications"
HICOLOR_DIR="$DATA_DIR/icons/hicolor"
ICON_DIR="$HICOLOR_DIR/scalable/apps"
STATE_DIR="$DATA_DIR/taskforest"
STATE_FILE="$STATE_DIR/dev-install-frontends.tsv"
ICON_SRC="$REPO_ROOT/packaging/linux/io.github.YellowWhiteBlackCat.TaskForest.svg"
ICON_DST="$ICON_DIR/taskforest-taskboard.svg"
HICOLOR_INDEX_SRC="${TASKFOREST_HICOLOR_INDEX_SOURCE:-/usr/share/icons/hicolor/index.theme}"
HICOLOR_INDEX_DST="$HICOLOR_DIR/index.theme"
GPUI_ID="io.github.YellowWhiteBlackCat.TaskForestG"
ICED_ID="io.github.YellowWhiteBlackCat.TaskForestI"
GPUI_DESKTOP_SRC="$REPO_ROOT/packaging/linux/$GPUI_ID.desktop"
ICED_DESKTOP_SRC="$REPO_ROOT/packaging/linux/$ICED_ID.desktop"
GPUI_DESKTOP_DST="$APPS_DIR/$GPUI_ID.desktop"
ICED_DESKTOP_DST="$APPS_DIR/$ICED_ID.desktop"
DEFAULT_BIN_DIR="$REPO_ROOT/target/frontend-binaries/release"
STATE_SCHEMA=2
LEGACY_STATE_SCHEMA=1
STAGE=""
RECEIPT_SCHEMA=""

declare -A RECEIPT_OWNER=()
declare -A RECEIPT_HASH=()

cleanup() {
  if [[ -n "$STAGE" && -d "$STAGE" && ! -L "$STAGE" ]]; then
    rm -rf -- "$STAGE"
  fi
}
trap cleanup EXIT

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

hash_file() {
  sha256sum -- "$1" | cut -d' ' -f1
}

path_exists() {
  [[ -e "$1" || -L "$1" ]]
}

require_regular_file() {
  local path="$1"
  [[ ! -L "$path" && -f "$path" ]] || die "refusing non-regular file: $path"
}

write_desktop_with_exec() {
  local source="$1" executable="$2" destination="$3"
  DESKTOP_EXEC="$executable" awk '
    BEGIN { count = 0 }
    /^Exec=/ { print "Exec=" ENVIRON["DESKTOP_EXEC"]; count += 1; next }
    { print }
    END { if (count != 1) exit 2 }
  ' "$source" >"$destination" || die "desktop template must contain exactly one Exec entry: $source"
}

load_receipt() {
  path_exists "$STATE_FILE" || return 1
  require_regular_file "$STATE_FILE"

  local kind owner digest extra
  local schema_seen=0
  while IFS=$'\t' read -r kind owner digest extra; do
    [[ -z "$extra" ]] || die "malformed ownership receipt: $STATE_FILE"
    if [[ "$kind" == "schema" ]]; then
      [[ "$schema_seen" -eq 0 \
        && ( "$owner" == "$STATE_SCHEMA" || "$owner" == "$LEGACY_STATE_SCHEMA" ) \
        && -z "$digest" ]] \
        || die "unsupported ownership receipt schema: $STATE_FILE"
      RECEIPT_SCHEMA="$owner"
      schema_seen=1
      continue
    fi
    [[ "$schema_seen" -eq 1 ]] \
      || die "ownership receipt schema must be the first entry: $STATE_FILE"
    [[ -z "${RECEIPT_OWNER[$kind]+present}" ]] \
      || die "duplicate ownership receipt entry '$kind': $STATE_FILE"
    case "$kind" in
      gpui-desktop | iced-desktop | shared-icon | hicolor-index) ;;
      *) die "unknown ownership receipt entry '$kind': $STATE_FILE" ;;
    esac
    case "$owner" in
      managed)
        [[ "$digest" =~ ^[0-9a-f]{64}$ ]] \
          || die "invalid managed hash for '$kind': $STATE_FILE"
        ;;
      external)
        [[ "$kind" == "hicolor-index" && "$digest" == "-" ]] \
          || die "invalid external ownership entry '$kind': $STATE_FILE"
        ;;
      *) die "invalid ownership kind '$owner' for '$kind': $STATE_FILE" ;;
    esac
    RECEIPT_OWNER[$kind]="$owner"
    RECEIPT_HASH[$kind]="$digest"
  done <"$STATE_FILE"

  [[ "$schema_seen" -eq 1 ]] || die "ownership receipt has no schema: $STATE_FILE"
  for kind in gpui-desktop iced-desktop shared-icon hicolor-index; do
    [[ -n "${RECEIPT_OWNER[$kind]+present}" ]] \
      || die "ownership receipt is missing '$kind': $STATE_FILE"
  done
  return 0
}

verify_recorded_file() {
  local kind="$1" path="$2"
  [[ "${RECEIPT_OWNER[$kind]}" == "managed" ]] \
    || die "internal ownership mismatch for '$kind'"
  path_exists "$path" || return 0
  require_regular_file "$path"
  local actual
  actual="$(hash_file "$path")"
  [[ "$actual" == "${RECEIPT_HASH[$kind]}" ]] \
    || die "managed file changed since installation; leaving it untouched: $path"
}

verify_install_target() {
  local desired="$1" destination="$2"
  path_exists "$destination" || return 0
  require_regular_file "$destination"
  cmp -s -- "$desired" "$destination" \
    || die "destination contains different content; refusing to overwrite: $destination"
}

publish_new_or_equal() {
  local desired="$1" destination="$2"
  if path_exists "$destination"; then
    verify_install_target "$desired" "$destination"
    return 0
  fi
  # STAGE lives below DATA_DIR, so this hard-link publication is same-filesystem
  # and fails instead of overwriting a file created after preflight.
  if ! ln -- "$desired" "$destination"; then
    path_exists "$destination" && verify_install_target "$desired" "$destination" \
      || die "could not publish managed file: $destination"
  fi
}

refresh_caches() {
  command -v update-desktop-database >/dev/null 2>&1 \
    && update-desktop-database "$APPS_DIR" || true
  command -v gtk-update-icon-cache >/dev/null 2>&1 \
    && gtk-update-icon-cache -f "$HICOLOR_DIR" || true
  if command -v kbuildsycoca6 >/dev/null 2>&1; then
    kbuildsycoca6 --noincremental >/dev/null 2>&1 || true
  elif command -v kbuildsycoca5 >/dev/null 2>&1; then
    kbuildsycoca5 --noincremental >/dev/null 2>&1 || true
  fi
}

uninstall_managed() {
  if ! load_receipt; then
    printf 'No TaskForestG/TaskForestI developer-install receipt at %s; nothing removed.\n' \
      "$STATE_FILE"
    return 0
  fi

  verify_recorded_file gpui-desktop "$GPUI_DESKTOP_DST"
  verify_recorded_file iced-desktop "$ICED_DESKTOP_DST"
  verify_recorded_file shared-icon "$ICON_DST"
  if [[ "${RECEIPT_OWNER[hicolor-index]}" == "managed" ]]; then
    verify_recorded_file hicolor-index "$HICOLOR_INDEX_DST"
  fi

  rm -f -- "$GPUI_DESKTOP_DST" "$ICED_DESKTOP_DST" "$ICON_DST"
  if [[ "${RECEIPT_OWNER[hicolor-index]}" == "managed" ]]; then
    rm -f -- "$HICOLOR_INDEX_DST"
  fi
  rm -f -- "$STATE_FILE"
  rmdir -- "$STATE_DIR" 2>/dev/null || true
  refresh_caches
  printf 'Removed receipt-owned TaskForestG/TaskForestI developer integration from %s\n' \
    "$DATA_DIR"
}

if [[ "${1:-}" == "--uninstall" ]]; then
  [[ $# -eq 1 ]] || die "--uninstall does not accept binary paths"
  uninstall_managed
  exit 0
fi

if [[ $# -gt 2 ]]; then
  printf 'usage: %s [taskforest-g] [taskforest-i]\n' "$0" >&2
  exit 2
fi
GPUI_BIN="${1:-$DEFAULT_BIN_DIR/taskforest-g}"
ICED_BIN="${2:-$DEFAULT_BIN_DIR/taskforest-i}"

for binary in "$GPUI_BIN" "$ICED_BIN"; do
  [[ -x "$binary" && ! -L "$binary" && -f "$binary" ]] \
    || die "binary not found, not executable, or not regular: $binary"
done

GPUI_BIN="$(realpath -e -- "$GPUI_BIN")"
ICED_BIN="$(realpath -e -- "$ICED_BIN")"
require_regular_file "$ICON_SRC"
require_regular_file "$GPUI_DESKTOP_SRC"
require_regular_file "$ICED_DESKTOP_SRC"

mkdir -p -- "$DATA_DIR"
STAGE="$(mktemp -d "$DATA_DIR/.taskforest-frontends.XXXXXX")"
chmod 700 "$STAGE"

write_desktop_with_exec "$GPUI_DESKTOP_SRC" "$GPUI_BIN" "$STAGE/gpui.desktop"
write_desktop_with_exec "$ICED_DESKTOP_SRC" "$ICED_BIN" "$STAGE/iced.desktop"
install -m 0644 -- "$ICON_SRC" "$STAGE/taskforest-taskboard.svg"
chmod 0644 "$STAGE/gpui.desktop" "$STAGE/iced.desktop"

receipt_exists=0
if load_receipt; then
  receipt_exists=1
  verify_recorded_file gpui-desktop "$GPUI_DESKTOP_DST"
  verify_recorded_file iced-desktop "$ICED_DESKTOP_DST"
  verify_recorded_file shared-icon "$ICON_DST"
fi

verify_install_target "$STAGE/gpui.desktop" "$GPUI_DESKTOP_DST"
verify_install_target "$STAGE/iced.desktop" "$ICED_DESKTOP_DST"
verify_install_target "$STAGE/taskforest-taskboard.svg" "$ICON_DST"

index_owner=external
index_hash=-
if [[ "$receipt_exists" -eq 1 ]]; then
  index_owner="${RECEIPT_OWNER[hicolor-index]}"
  index_hash="${RECEIPT_HASH[hicolor-index]}"
  if [[ "$index_owner" == "managed" ]]; then
    verify_recorded_file hicolor-index "$HICOLOR_INDEX_DST"
  elif ! path_exists "$HICOLOR_INDEX_DST"; then
    die "previously external hicolor index disappeared; uninstall the receipt before taking ownership"
  fi
elif path_exists "$HICOLOR_INDEX_DST"; then
  require_regular_file "$HICOLOR_INDEX_DST"
fi

if [[ "$index_owner" == "managed" ]] || ! path_exists "$HICOLOR_INDEX_DST"; then
  require_regular_file "$HICOLOR_INDEX_SRC"
  install -m 0644 -- "$HICOLOR_INDEX_SRC" "$STAGE/index.theme"
  verify_install_target "$STAGE/index.theme" "$HICOLOR_INDEX_DST"
  index_owner=managed
  index_hash="$(hash_file "$STAGE/index.theme")"
fi

{
  printf 'schema\t%s\n' "$STATE_SCHEMA"
  printf 'gpui-desktop\tmanaged\t%s\n' "$(hash_file "$STAGE/gpui.desktop")"
  printf 'iced-desktop\tmanaged\t%s\n' "$(hash_file "$STAGE/iced.desktop")"
  printf 'shared-icon\tmanaged\t%s\n' "$(hash_file "$STAGE/taskforest-taskboard.svg")"
  printf 'hicolor-index\t%s\t%s\n' "$index_owner" "$index_hash"
} >"$STAGE/dev-install-frontends.tsv"
chmod 0600 "$STAGE/dev-install-frontends.tsv"

if [[ "$receipt_exists" -eq 1 ]]; then
  cmp -s -- "$STAGE/dev-install-frontends.tsv" "$STATE_FILE" \
    || die "owned installation differs from this request; uninstall it before reinstalling"
fi

mkdir -p -- "$APPS_DIR" "$ICON_DIR" "$STATE_DIR"
publish_new_or_equal "$STAGE/gpui.desktop" "$GPUI_DESKTOP_DST"
publish_new_or_equal "$STAGE/iced.desktop" "$ICED_DESKTOP_DST"
publish_new_or_equal "$STAGE/taskforest-taskboard.svg" "$ICON_DST"
if [[ "$index_owner" == "managed" ]]; then
  publish_new_or_equal "$STAGE/index.theme" "$HICOLOR_INDEX_DST"
fi
publish_new_or_equal "$STAGE/dev-install-frontends.tsv" "$STATE_FILE"

refresh_caches
printf 'Installed receipt-owned developer entries:\n  %s (Exec=%s)\n  %s (Exec=%s)\n  %s\n  receipt=%s\n' \
  "$GPUI_DESKTOP_DST" "$GPUI_BIN" "$ICED_DESKTOP_DST" "$ICED_BIN" \
  "$ICON_DST" "$STATE_FILE"
