#!/usr/bin/env bash
# bevy 行特有门禁(verify-isolated.sh all 档在 worktree 内执行)。
# fail-loud:任何门禁失败必须把尾部输出带回调用者,禁止静默失败。
# 注意: deliberately以调用者 cwd 为准(= 隔离 worktree),绝不锚回脚本
# 所在路径——那会跳回共享 checkout,把兄弟线的撕裂状态当成门禁对象。
set -uo pipefail

run_gate() {
    local name="$1"; shift
    local out status
    if out=$("$@" 2>&1); then
        status=0
    else
        status=$?
    fi
    printf '%s\n' "$out" | tail -4
    if [[ "$status" -ne 0 ]]; then
        printf '%s\n' "$out" > /tmp/isolated-bevy-$name-failed.log
        printf 'ISOLATED[bevy] %s: FAIL (exit %s, full output: /tmp/isolated-bevy-%s-failed.log)\n' "$name" "$status" "$name" >&2
        return 1
    fi
    printf 'ISOLATED[bevy] %s: PASS\n' "$name"
}

run_gate "bsn-guard" timeout --kill-after=10s 120s \
    python3 scripts/quality/bevy_bsn_guard.py --mode enforce
run_gate "interactions" bash scripts/accept-bevy-interactions.sh
