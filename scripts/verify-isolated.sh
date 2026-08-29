#!/usr/bin/env bash
# Isolated per-line verification: run a line's gates against its own diff on
# top of HEAD, inside a private worktree, so another line's torn in-flight
# edits in the shared checkout can never block this line's gates.
#
# 背景（AGENTS.md 并行纪律）：每个 UI/平台线在共享 checkout 里并行推进；
# 兄弟线未提交的重构（哪怕暂时编译不过）只属于兄弟线。本脚本把本行的
# 未提交差异（含 intent-to-add 的未跟踪文件）应用到 HEAD worktree 上，
# 在那里跑本行门禁——主 checkout 永不被本脚本改动。
#
# 用法:
#   scripts/verify-isolated.sh <line> [gate]
#     <line>  行名；路径清单在 scripts/isolated-paths/<line>.txt
#             （每行一个 git pathspec，'#' 开头为注释；行足迹变化时同步更新）
#     [gate]  test | clippy | fmt | capture | all   （默认 all）
#
# 可选: scripts/isolated-paths/<line>.extra-gates.sh —— 若存在，`all` 时在
# worktree 内执行（行特有门禁，如交互矩阵、bsn 结构守卫）。
#
# Cargo 锁策略与全仓一致: TM_CARGO_LOCK 设为空 = 解锁（dev 阶段回退），
# 未设置 = --locked。
set -euo pipefail

LINE="${1:?usage: verify-isolated.sh <line> [test|clippy|fmt|capture|all]}"
GATE="${2:-all}"
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PATHS_FILE="$REPO/scripts/isolated-paths/$LINE.txt"
EXTRA_GATES="$REPO/scripts/isolated-paths/$LINE.extra-gates.sh"
WT="$REPO/.tmp/isolated-$LINE"
# 共享 target:依赖图只编译一次,各行复用(Cargo 锁本身串行化并发构建)。
export CARGO_TARGET_DIR="$REPO/target"
# 隔离 worktree 的 Cargo.lock 落后于本线依赖清单(HEAD 基线 + 行补丁),
# 首次构建必须解锁回填;此后保持稳定。调用方可显式覆盖。
export TM_CARGO_LOCK="${TM_CARGO_LOCK-}"

[[ -s "$PATHS_FILE" ]] || { printf 'missing path list: %s\n' "$PATHS_FILE" >&2; exit 2; }

# 路径/包清单以数组承载(引用展开,无字符串拼接,无注入面)。
pathspec() { grep -v '^\s*#' "$PATHS_FILE" | grep -v '^\s*$' | grep -v '^package:'; }
pathspec_args() { pathspec | tr '\n' '\0' | xargs -0 printf '%s\0'; }

# 声明本行的 cargo 包(可多行 `package: <name> <name> ...`);test/clippy
# 门禁只构建这些包,避免把兄弟线撕裂的无关包卷进本行验证。
packages() {
    sed -n 's/^package:[[:space:]]*//p' "$PATHS_FILE" | tr ' ' '\n' | grep -v '^\s*$'
}

# 未跟踪文件经 intent-to-add 进入 diff;结束前恢复索引,不污染兄弟线的
# git status 视图。
stage_intent() {
    while IFS= read -r p; do
        git add -N -- "$p" 2>/dev/null || true
    done < <(pathspec)
}
unstage_intent() {
    while IFS= read -r p; do
        # 显式 -C 主仓库:本函数在脚本退出陷阱里执行,此时 cwd 可能已是
        # worktree,不带 -C 会清错仓库的索引。
        git -C "$REPO" reset -q -- "$p" 2>/dev/null || true
    done < <(pathspec)
}

cd "$REPO"
# wt 保留在磁盘上(.tmp 下,gitignored):capture 的验收证据存在里面,
# 由下一次运行的 rm -rf 兜底重置,而不是本次退出时销毁。
trap 'unstage_intent' EXIT

stage_intent
mapfile -t PATH_ARGS < <(pathspec)
git diff --binary HEAD -- "${PATH_ARGS[@]}" > "$REPO/.tmp/isolated-$LINE.patch"
unstage_intent

git worktree remove --force "$WT" 2>/dev/null || true
# 残留注册表/半删除状态兜底:先清目录再 prune,保证 add 一定成功。
rm -rf "$WT"
git worktree prune
git worktree add "$WT" --detach HEAD >/dev/null
git -C "$WT" apply "$REPO/.tmp/isolated-$LINE.patch"

# 行内 fmt 归一:只格式化本行包,避免解析兄弟线未提交/缺失的文件;
# 兄弟面的漂移不是本行门禁的对象。
mapfile -t PKG_ARGS < <(packages)
# 展开成独立的 -p <name> 参数对,全程引用传递。
PKG_FLAGS=()
for pkg in "${PKG_ARGS[@]}"; do
    PKG_FLAGS+=(-p "$pkg")
done
cargo fmt "${PKG_FLAGS[@]}" 2>/dev/null || true

# 锁落定:解锁回填本线依赖清单对 HEAD 锁的增量,必须在任何"记录 git 状态
# → 校验状态一致"的流程(capture validator)之前完成,否则首次构建改锁会
# 造成状态漂移假失败。
cd "$WT"
cargo build "${PKG_FLAGS[@]}" >/dev/null 2>&1 || true

run_fmt()      { cargo fmt --check "${PKG_FLAGS[@]}" && echo "ISOLATED[$LINE] fmt: PASS"; }
run_clippy()   { cargo clippy "${PKG_FLAGS[@]}" --all-targets 2>&1 | grep -qE '^warning: |^error' && {
                     cargo clippy "${PKG_FLAGS[@]}" --all-targets 2>&1 | grep -E '^warning: |^error' -A 4 | head -40
                     return 1
                 }; echo "ISOLATED[$LINE] clippy: PASS"; }
run_test()     { cargo nextest run "${PKG_FLAGS[@]}" --all-targets -j 4 && echo "ISOLATED[$LINE] test: PASS"; }
run_capture()  {
    # capture 自带 wt-local target(二进制隔离);结束后把证据目录拷回主仓
    # target,避免隔离 worktree 的生命周期带走验收证据。
    (unset CARGO_TARGET_DIR; bash scripts/capture-$LINE.sh 2>&1 | tail -2)
    local latest
    latest="$(ls -dt "$WT/target/bevy-evidence"/*/ 2>/dev/null | head -1 || true)"
    if [[ -n "$latest" ]]; then
        mkdir -p "$REPO/target/bevy-evidence-isolated"
        cp -r "${latest%/}" "$REPO/target/bevy-evidence-isolated/"
        printf 'evidence preserved: target/bevy-evidence-isolated/%s\n' "$(basename "$latest")"
    fi
}

case "$GATE" in
    fmt)      run_fmt ;;
    clippy)   run_clippy ;;
    test)     run_test ;;
    capture)  run_capture ;;
    all)
        run_fmt
        run_test
        run_clippy
        if [[ -f "$EXTRA_GATES" ]]; then
            bash "$EXTRA_GATES"
        fi
        ;;
    *) printf 'unknown gate: %s\n' "$GATE" >&2; exit 2 ;;
esac

echo "ISOLATED[$LINE] $GATE: DONE"
