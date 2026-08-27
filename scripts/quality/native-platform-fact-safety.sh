#!/usr/bin/env bash
# Native macOS/Windows fact-safety receipt. This must run on the real target;
# cross-compilation is useful evidence, but cannot prove native API behavior.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

kernel="$(uname -s)"
case "$kernel" in
    Darwin)
        native_platform="macos"
        packages=(
            -p taskmanager-platform-portable
            -p taskmanager-platform-macos
        )
        ;;
    MINGW*|MSYS*|CYGWIN*)
        native_platform="windows"
        packages=(
            -p taskmanager-platform-portable
            -p taskmanager-platform-windows
            -p taskmanager-windows-api
        )
        ;;
    *)
        printf 'SKIP native-platform-fact-safety: host %s is neither macOS nor Windows\n' "$kernel"
        exit 0
        ;;
esac

run_id="$(date -u +%Y%m%dT%H%M%SZ)"
run_dir="$repo_root/.tmp/native-platform-fact-safety/$native_platform-$run_id"
mkdir -p "$run_dir"
log="$run_dir/nextest.log"
receipt="$run_dir/receipt.txt"

if cargo nextest run --locked "${packages[@]}" --all-targets -j 4 >"$log" 2>&1; then
    result="PASS"
else
    result="FAIL"
fi

{
    printf 'schema=taskforest.native-platform-fact-safety.v1\n'
    printf 'platform=%s\n' "$native_platform"
    printf 'kernel=%s\n' "$kernel"
    printf 'git_head=%s\n' "$(git rev-parse HEAD)"
    if [[ -n "$(git status --porcelain)" ]]; then
        printf 'worktree=dirty\n'
    else
        printf 'worktree=clean\n'
    fi
    printf 'command=cargo nextest run --locked %s --all-targets -j 4\n' "${packages[*]}"
    printf 'result=%s\n' "$result"
    printf 'log=%s\n' "$log"
} >"$receipt"

cat "$log"
cat "$receipt"
[[ "$result" == "PASS" ]]
