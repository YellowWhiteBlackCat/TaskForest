#!/usr/bin/env bash
# Build the frontend products into independently runnable artifacts.
#
# Each frontend is its own product crate with its own binary (ADR-051): the
# builds share the repository's common target directory without colliding,
# and every supported frontend artifact lands in OUTPUT_DIR ready for
# installation and testing together.
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

# ADR-051: each product is its own crate + bin; no feature switching and no
# artifact collisions in target/<profile>/.
cargo build --locked -p taskmanager-gpui "${PROFILE_ARGS[@]}"
install -Dm755 "target/$CARGO_PROFILE_DIR/taskforest-g$EXE_SUFFIX" \
  "$OUTPUT_DIR/taskforest-g$EXE_SUFFIX"

cargo build --locked -p taskmanager-iced "${PROFILE_ARGS[@]}"
install -Dm755 "target/$CARGO_PROFILE_DIR/taskforest-i$EXE_SUFFIX" \
  "$OUTPUT_DIR/taskforest-i$EXE_SUFFIX"

# Bevy frontend (docs/BEVY_UI_FRONTEND.md): its own product crate, own bin
# name, Wayland-only bevy closure on Linux.
cargo build --locked -p taskmanager-bevy-ui "${PROFILE_ARGS[@]}"
install -Dm755 "target/$CARGO_PROFILE_DIR/taskforest-b$EXE_SUFFIX" \
  "$OUTPUT_DIR/taskforest-b$EXE_SUFFIX"

# TUI frontend: its own product crate, own bin name (taskforest-t).
cargo build --locked -p taskmanager-tui "${PROFILE_ARGS[@]}"
install -Dm755 "target/$CARGO_PROFILE_DIR/taskforest-t$EXE_SUFFIX" \
  "$OUTPUT_DIR/taskforest-t$EXE_SUFFIX"

printf 'product binaries ready:\n  %s\n  %s\n  %s\n  %s\n' \
  "$OUTPUT_DIR/taskforest-g$EXE_SUFFIX" "$OUTPUT_DIR/taskforest-i$EXE_SUFFIX" \
  "$OUTPUT_DIR/taskforest-b$EXE_SUFFIX" "$OUTPUT_DIR/taskforest-t$EXE_SUFFIX"
