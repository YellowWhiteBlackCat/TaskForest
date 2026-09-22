#!/usr/bin/env bash
# Blocking headless regression gate for the four shipped frontend products.
#
# This is intentionally a single explicit nextest invocation. The workspace
# gate also exercises dependencies, but this package filter makes the product
# boundary auditable: GPUI, Iced, TUI and Bevy must each have discovered and
# passing tests before the merge gate can report success.

set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"

export CARGO_BUILD_JOBS="${JOBS:-4}"
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"

timeout --kill-after=30s "${FOUR_FRONTENDS_TIMEOUT:-3600}" \
    cargo nextest run --locked --workspace --all-targets --features test-support -j 4 \
    --no-fail-fast \
    -E 'package(taskmanager-gpui) | package(taskmanager-iced) | package(taskmanager-tui) | package(taskmanager-bevy-ui)'

echo "four-frontends-nextest: PASS"
