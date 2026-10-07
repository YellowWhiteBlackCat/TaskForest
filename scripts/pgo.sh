#!/usr/bin/env bash
# =============================================================================
# PGO（Profile-Guided Optimization）—— **opt-in 本地提速工具**，不是门禁。
#
# 为什么不挂进 `scripts/quality/local-gates.sh` / nextest / CI：
#   * 三趟构建 + 训练远超任何门禁 deadline；
#   * `.profdata` 与本机 CPU、本仓 rustc/LLVM 版本强绑定，绝不入库；profile 与
#     产物都落在 `target/`（已被忽略）。
# 需要更快的 release 产物时手动跑：
#
#     bash scripts/pgo.sh product     # 默认：用 parser 基准训练，优化 taskforest-g
#     bash scripts/pgo.sh list
#
# 训练负载：`taskmanager-platform-linux` 的零依赖吞吐基准
# （`benches/throughput.rs`，即 `scripts/quality/bench-gate.sh` 跑的那支）。
# 它确定性、无 GUI，覆盖 `/proc` 热解析器；这些解析器也编进产品二进制，
# 因此 bench 采集的 profile 对产品共享函数有效（未命中的函数按常规编译，不报错）。
#
# 环境开关：
#     PGO_STRICT=1   用 profile 重编时加 `-pgo-warn-missing-function`
#     PGO_KEEP=1     保留中间 `.profraw`
#     CARGO_TARGET_DIR=<dir>   覆盖 target 目录
# =============================================================================
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

# -----------------------------------------------------------------------------
# 共享内核（各仓逐字复制；改动请同步 dongxinanbei/tools/pgo.sh、
# taskmanager/scripts/pgo.sh、ZhuGeTower/scripts/performance/pgo.sh）
# -----------------------------------------------------------------------------
log() { printf '[pgo] %s\n' "$*" >&2; }
die() { printf '[pgo] ERROR: %s\n' "$*" >&2; exit 1; }

find_llvm_profdata() {
  local host sysroot candidate
  host="$(rustc -vV | awk '/^host:/{print $2}')"
  sysroot="$(rustc --print sysroot)"
  for candidate in \
    "$sysroot/lib/rustlib/$host/bin/llvm-profdata" \
    "$sysroot/lib/rustlib/$host/bin/llvm-profdata.exe"; do
    [[ -x "$candidate" ]] && { printf '%s' "$candidate"; return 0; }
  done
  for candidate in llvm-profdata llvm-profdata-23 llvm-profdata-22 llvm-profdata-21; do
    if command -v "$candidate" >/dev/null 2>&1; then command -v "$candidate"; return 0; fi
  done
  return 1
}

pgo_execute() {
  : "${PGO_NAME:?}" "${PGO_TARGET_DIR:?}" "${PGO_PROFILE_KEY:?}" \
    "${PGO_OUT_DIR:?}" "${PGO_MANIFEST:?}" "${PGO_GEN:?}" "${PGO_USE:?}" \
    "${PGO_TRAIN:?}" "${PGO_OUT_BINS:?}"

  local profdata rustc_llvm tool_llvm
  profdata="$(find_llvm_profdata)" \
    || die "找不到 llvm-profdata：请 rustup component add llvm-tools，或安装系统 llvm 23"
  rustc_llvm="$(rustc -vV | awk '/^LLVM version:/{print $3}')"
  tool_llvm="$("$profdata" --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)"
  if [[ -n "$rustc_llvm" && -n "$tool_llvm" && "${rustc_llvm%%.*}" != "${tool_llvm%%.*}" ]]; then
    log "警告：llvm-profdata=$tool_llvm 与 rustc 的 LLVM=$rustc_llvm 主版本不同，profile 可能不兼容"
  fi

  local profdir="$PGO_TARGET_DIR/pgo/$PGO_NAME"
  local rawdir="$profdir/raw"
  local merged="$profdir/merged.profdata"
  local outbin="$profdir/bin"
  mkdir -p "$rawdir" "$outbin"
  find "$rawdir" -name '*.profraw' -delete 2>/dev/null || true
  rm -f "$merged"

  local -a common=(--manifest-path "$PGO_MANIFEST")
  [[ -n "${PGO_CARGO_CONFIG:-}" ]] && common+=(--config "$PGO_CARGO_CONFIG")

  log "recipe=$PGO_NAME profile=$PGO_PROFILE_KEY target-dir=$PGO_TARGET_DIR"
  log "stage 1/3 插桩构建：cargo ${PGO_GEN[*]}"
  env \
    "RUSTFLAGS=${RUSTFLAGS:-} -Cprofile-generate=$rawdir" \
    "CARGO_TARGET_DIR=$PGO_TARGET_DIR" \
    "CARGO_PROFILE_${PGO_PROFILE_KEY}_LTO=false" \
    "CARGO_PROFILE_${PGO_PROFILE_KEY}_CODEGEN_UNITS=16" \
    cargo "${PGO_GEN[@]}" "${common[@]}"

  log "stage 2/3 训练负载"
  local cmd
  for cmd in "${PGO_TRAIN[@]}"; do
    [[ -n "$cmd" ]] || continue
    log "  train: $cmd"
    ( cd "$PROJECT_ROOT" && \
      env "CARGO_TARGET_DIR=$PGO_TARGET_DIR" "PGO_OUT_DIR=$PGO_OUT_DIR" "PGO_RAW_DIR=$rawdir" \
      bash -euo pipefail -c "$cmd" )
  done

  local count
  count="$(find "$rawdir" -name '*.profraw' | wc -l | tr -d ' ')"
  (( count > 0 )) || die "训练没有产生任何 .profraw（训练命令是否真的执行了插桩二进制？）"
  log "  收集到 $count 个 .profraw"

  log "stage 3/3 合并 profile + 用 profile 重编（走仓库正式 release 档）"
  "$profdata" merge -o "$merged" "$rawdir"
  local use_flags="-Cprofile-use=$merged"
  [[ "${PGO_STRICT:-0}" = "1" ]] && use_flags="$use_flags -Cllvm-args=-pgo-warn-missing-function"
  env \
    "RUSTFLAGS=${RUSTFLAGS:-} $use_flags" \
    "CARGO_TARGET_DIR=$PGO_TARGET_DIR" \
    cargo "${PGO_USE[@]}" "${common[@]}"

  local bin
  for bin in "${PGO_OUT_BINS[@]}"; do
    [[ -f "$PGO_OUT_DIR/$bin" ]] || die "期望的产物不存在：$PGO_OUT_DIR/$bin"
    cp -f "$PGO_OUT_DIR/$bin" "$outbin/"
    log "  产物：$outbin/$bin"
  done
  [[ "${PGO_KEEP:-0}" = "1" ]] || find "$rawdir" -name '*.profraw' -delete 2>/dev/null || true
  log "完成。profile=$merged"
}

# -----------------------------------------------------------------------------
# Recipe 区
# -----------------------------------------------------------------------------
recipe_product() {
  PGO_NAME="product"
  PGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PROJECT_ROOT/target}"
  PGO_PROFILE_KEY="BENCH"          # 训练用 `cargo bench`，故插桩档是 bench
  PGO_OUT_DIR="$PGO_TARGET_DIR/release"
  PGO_MANIFEST="$PROJECT_ROOT/Cargo.toml"
  PGO_CARGO_CONFIG=""
  PGO_GEN=(bench -p taskmanager-platform-linux --bench throughput --features test-support)
  PGO_USE=(build --release -p taskmanager-gpui)
  PGO_TRAIN=()                     # `cargo bench` 本身即训练
  PGO_OUT_BINS=(taskforest-g)
  pgo_execute
}

usage() {
  cat <<'USAGE'
用法：bash scripts/pgo.sh <recipe>

recipes:
  product  用 platform-linux 吞吐基准训练，优化 taskforest-g（GPUI 产品二进制）
  list     列出 recipes

环境：PGO_STRICT=1 诊断缺失函数；PGO_KEEP=1 保留 .profraw；CARGO_TARGET_DIR 覆盖 target。
USAGE
}

case "${1:-product}" in
  product) recipe_product ;;
  list) usage ;;
  -h|--help|help) usage ;;
  *) usage; die "未知 recipe：$1" ;;
esac
