#!/usr/bin/env bash
# Build the desktop frontend shapes into independently runnable artifacts.
#
# Cargo's root package intentionally compiles exactly one UI shape at a time.
# This script uses the repository's shared target directory, copies each
# completed shape before the next feature build replaces target/<profile>/taskmanager,
# and builds the standalone Bevy UI crate under its own bin name, leaving all
# supported frontend artifacts available for installation and testing together.
#
# Usage:
#   scripts/build-frontend-binaries.sh              # release artifacts
#   scripts/build-frontend-binaries.sh debug        # debug artifacts
#   scripts/build-frontend-binaries.sh release DIR  # explicit output directory
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="${1:-release}"
OUTPUT_DIR="${2:-$REPO_ROOT/target/frontend-binaries/$PROFILE}"

case "$PROFILE" in
  debug)
    PROFILE_ARGS=()
    CARGO_PROFILE_DIR="debug"
    ;;
  release)
    PROFILE_ARGS=(--release)
    CARGO_PROFILE_DIR="release"
    ;;
  *)
    echo "usage: $0 [debug|release] [output-dir]" >&2
    exit 2
    ;;
esac

cd "$REPO_ROOT"
mkdir -p "$OUTPUT_DIR"

EXE_SUFFIX=""
case "$(uname -s)" in
  MINGW* | MSYS* | CYGWIN*) EXE_SUFFIX=".exe" ;;
esac

cargo build --locked -p taskmanager "${PROFILE_ARGS[@]}" \
  --no-default-features --features hardware-all,ui-gpui
install -Dm755 "target/$CARGO_PROFILE_DIR/taskmanager$EXE_SUFFIX" \
  "$OUTPUT_DIR/taskforest-g$EXE_SUFFIX"

cargo build --locked -p taskmanager "${PROFILE_ARGS[@]}" \
  --no-default-features --features hardware-all,ui-iced
install -Dm755 "target/$CARGO_PROFILE_DIR/taskmanager$EXE_SUFFIX" \
  "$OUTPUT_DIR/taskforest-i$EXE_SUFFIX"

# Third frontend (docs/BEVY_UI_FRONTEND.md): standalone crate, own bin name,
# Wayland-only bevy closure on Linux — no root-package feature switching.
cargo build --locked -p taskmanager-bevy-ui "${PROFILE_ARGS[@]}"
install -Dm755 "target/$CARGO_PROFILE_DIR/taskforest-b$EXE_SUFFIX" \
  "$OUTPUT_DIR/taskforest-b$EXE_SUFFIX"

printf 'product binaries ready:\n  %s\n  %s\n  %s\n' \
  "$OUTPUT_DIR/taskforest-g$EXE_SUFFIX" "$OUTPUT_DIR/taskforest-i$EXE_SUFFIX" \
  "$OUTPUT_DIR/taskforest-b$EXE_SUFFIX"
