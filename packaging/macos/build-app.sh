#!/usr/bin/env bash
# Assemble a side-by-side TaskForestG.app or TaskForestI.app from a cargo-built
# frontend binary.
#
# Mechanism: the .app is a plain directory tree; macOS LaunchServices reads
# Contents/Info.plist -> CFBundleIconFile -> Contents/Resources/icon.icns for
# the Dock/Launchpad icon (see Info.plist header for the gpui-source rationale).
# This script needs only coreutils (mkdir/install/cp) + the prebuilt icon.icns
# that ships next to it. No cargo-bundle, no iconutil, no Xcode.
#
# Usage:
#   packaging/macos/build-app.sh                 # packages target/release/taskforest-g
#   packaging/macos/build-app.sh path/to/bin     # package an explicit binary
#   TASKFOREST_FRONTEND=iced packaging/macos/build-app.sh
#   OUT_DIR=dist packaging/macos/build-app.sh    # write the .app somewhere else
#
# The binary MUST be a macOS build (cargo build --release --target ...-apple-darwin).
# The script will happily package a Linux/Windows binary too, but the result
# will not launch on macOS; it does not check the Mach-O type on purpose so it
# stays dependency-free.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

FRONTEND="${TASKFOREST_FRONTEND:-gpui}"
case "$FRONTEND" in
    gpui)
        APP_NAME="TaskForestG"
        BIN_NAME="taskforest-g"
        PLIST_SOURCE="${SCRIPT_DIR}/Info.plist"
        ;;
    iced)
        APP_NAME="TaskForestI"
        BIN_NAME="taskforest-i"
        PLIST_SOURCE="${SCRIPT_DIR}/Info-Iced.plist"
        ;;
    *)
        echo "error: TASKFOREST_FRONTEND must be gpui or iced" >&2
        exit 2
        ;;
esac
OUT_DIR="${OUT_DIR:-${REPO_ROOT}/target}"
BIN_PATH="${1:-${REPO_ROOT}/target/release/${BIN_NAME}}"

APP_ROOT="${OUT_DIR}/${APP_NAME}.app"
CONTENTS="${APP_ROOT}/Contents"
MACOS_DIR="${CONTENTS}/MacOS"
RESOURCES="${CONTENTS}/Resources"

if [[ ! -f "${BIN_PATH}" ]]; then
    echo "error: binary not found: ${BIN_PATH}" >&2
    echo "       build it first, e.g.:" >&2
    echo "         scripts/build-frontend-binaries.sh release" >&2
    echo "       or pass the path: $0 path/to/${BIN_NAME}" >&2
    exit 1
fi
if [[ ! -s "${SCRIPT_DIR}/icon.icns" ]]; then
    echo "error: TaskForest bundle icon is missing or empty: ${SCRIPT_DIR}/icon.icns" >&2
    exit 1
fi

echo "==> Packaging ${APP_NAME}.app from ${BIN_PATH}"

rm -rf "${APP_ROOT}"
mkdir -p "${MACOS_DIR}" "${RESOURCES}"

# Executable.
install -m755 "${BIN_PATH}" "${MACOS_DIR}/${BIN_NAME}"

# Bundle metadata.
install -m644 "${PLIST_SOURCE}" "${CONTENTS}/Info.plist"
install -m644 "${SCRIPT_DIR}/PkgInfo"  "${CONTENTS}/PkgInfo"

# Icon (CFBundleIconFile=icon -> Resources/icon.icns).
install -m644 "${SCRIPT_DIR}/icon.icns" "${RESOURCES}/icon.icns"
[[ -s "${RESOURCES}/icon.icns" ]] || {
    echo "error: packaged TaskForest bundle icon is missing or empty" >&2
    exit 1
}

echo "==> ${APP_ROOT}"
echo "    ${MACOS_DIR}/${BIN_NAME}"
echo "    ${CONTENTS}/Info.plist"
echo "    ${CONTENTS}/PkgInfo"
echo "    ${RESOURCES}/icon.icns"
echo
echo "Install (optional):"
echo "  rm -rf /Applications/${APP_NAME}.app && cp -R \"${APP_ROOT}\" /Applications/"
echo "Launch:"
echo "  open \"${APP_ROOT}\""
