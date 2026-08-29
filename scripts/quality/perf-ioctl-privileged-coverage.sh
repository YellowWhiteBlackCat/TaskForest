#!/usr/bin/env bash
# perf-ioctl 特权补测程序（留痕脚本：只打印，不执行）。
#
# 背景：docs/QUALITY_GATES.md §3 —— floor 68.4 播种于内嵌测试旧口径；8e58011e 迁出
# 测试后非特权宿主上限 50.0%（成功路径在 perf_event_open 之后，perf_event_paranoid=2
# 无 CAP 即 EACCES）。现有测试只断言失败路径：不先补成功路径测试，特权运行测得的
# 仍是 50.0%。本脚本把 owner 手工执行的完整序列留在版本库；运行它只会打印步骤。
# 陷阱提醒：步骤 0 的前置编辑（成功路径测试 + TASKFOREST_PRIVILEGED_PERF=1 早退门）
# 尚不存在，跳过它特权运行毫无意义。回执之后的 floor 裁决仍归 owner（见 §3）。
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo"
cat <<'PLAN'
=== perf-ioctl 特权补测程序（未执行；owner 在 bash 手工执行） ===
# 0) 前置编辑（尚无）：tests/headless/perf_contract.rs 增加成功路径测试
#    （软件 cpu-clock 事件 open→read_counter→enable→disable→reset 全链）；
#    未设 TASKFOREST_PRIVILEGED_PERF=1 时打印原因并早退（诚实 skip，不假成功）。
repo=$(git rev-parse --show-toplevel)
eval "$(scripts/agent-workdir.sh enter perf-priv-cov)"
# 1) 特权测量；隔离 target 目录，避免 root 属主文件进入共享 target/
sudo env PATH="$PATH" CARGO_HOME="$HOME/.cargo" RUSTUP_HOME="$HOME/.rustup" \\
     CARGO_TARGET_DIR="$repo/.tmp/perf-priv-target" \\
     LLVM_PROFILE_FILE="$repo/.tmp/perf-priv-%p-%m.profraw" \\
     TASKFOREST_PRIVILEGED_PERF=1 \\
     cargo llvm-cov nextest --locked -p taskmanager-perf-ioctl --all-targets -j 4 \\
       --profile ci --lcov --output-path "$repo/.tmp/lcov-perf-privileged.info"
# 2) 产物属主归还普通用户
sudo chown -R "$(id -u):$(id -g)" "$repo/.tmp/lcov-perf-privileged.info" "$repo/.tmp/perf-priv-target"
# 3) 只读基线，不改 floors
timeout 120s python3 scripts/quality/per_crate_coverage_gate.py --lcov .tmp/lcov-perf-privileged.info --baseline
# 4) 回执只入本机 `.private/`：perf_event_paranoid、kernel、CAP、逐字数字；
#    不把主机证据、内核细节或覆盖趋势提交到公开文档。
PLAN
echo "(dry-run by design: this script prints the plan and exits)"
