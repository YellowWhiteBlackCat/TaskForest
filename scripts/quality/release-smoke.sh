#!/usr/bin/env bash
# Linux release-shape smoke shared by local standard gates and blocking CI.
#
# This deliberately validates the artifact set before packaging workflows run:
# all release binaries are built, the isolated install manager is exercised,
# and the Arch package() layout is replayed without touching /usr or /etc.
# Set PR_SMOKE_PROFILE=true for the fast PR profile used by ci.yml; omit it to
# build the shipping release profile.

set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"

case "$(uname -s)" in
Linux*) ;;
*)
    echo "release-smoke is Linux-only (uname: $(uname -s))" >&2
    exit 2
    ;;
esac

# Direct invocations should retain the same moving stable channel and warning
# policy as the local gate and Linux CI caller. Caller-provided linker flags
# (including CI's mold flag) are preserved.
export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-line-tables-only}"
rustflags="${RUSTFLAGS:-}"
if [[ "$rustflags" != *"-D warnings"* ]]; then
    rustflags="${rustflags:+$rustflags }-D warnings"
fi
export RUSTFLAGS="$rustflags"

jobs="${JOBS:-4}"
build_timeout="${RELEASE_BUILD_TIMEOUT:-3600}"
if [[ "${PR_SMOKE_PROFILE:-false}" == "true" ]]; then
    export CARGO_PROFILE_RELEASE_LTO="${CARGO_PROFILE_RELEASE_LTO:-off}"
    export CARGO_PROFILE_RELEASE_CODEGEN_UNITS="${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-16}"
fi

timeout --kill-after=30s "$build_timeout" cargo build --locked --release -j "$jobs" \
    -p taskmanager-gpui \
    -p taskmanager-setup-helper \
    -p taskmanager-privilege-helper \
    -p taskmanager-net-launcher \
    -p taskmanager-process-control-helper \
    -p taskmanager-smbios-helper \
    -p taskmanager-rapl-helper \
    -p taskmanager-msr-helper

# The GPUI product binary is emitted as target/release/taskforest-g directly
# (ADR-051); no artifact rename step is needed.
timeout --kill-after=10s 30s scripts/test-system-install-manager.sh
timeout --kill-after=30s 120s packaging/arch/stage-package-sim.sh

echo "release-smoke: PASS"
