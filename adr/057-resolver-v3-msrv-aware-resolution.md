# ADR-057: 依赖解析器升级到 resolver v3（MSRV 感知解析）

Status: accepted

- 相关：根 `Cargo.toml`、`rust-toolchain.toml`、
  [QUALITY_GATES.md](../docs/QUALITY_GATES.md)

## 背景

根包使用 edition 2024，其默认依赖解析器已是 v3，但工作区此前**显式**钉在
`resolver = "2"`。resolver v2 与 v3 只有一处差别：`resolver.incompatible-rust-versions`
的默认值从 `allow` 变为 `fallback`。`allow` 允许依赖解析选中 `rust-version` 高于本包
floor 的版本；`fallback` 让解析优先选择 `rust-version` 不超过本包 floor 的版本。

本仓库把 `Cargo.toml` 的 `rust-version = "1.99.0"` 写成对外的兼容性承诺，
`rust-toolchain.toml` 固定同一工具链。在 `resolver = "2"` 下，这个承诺在依赖解析
阶段不成立：一次普通 `cargo update` 就可能把 MSRV 高于 1.99.0 的依赖版本拉进来，
而锁文件固定的是依赖版本、不是工具链。

## 决策

1. 根 `Cargo.toml` 的 `resolver` 升级为 `"3"`，让依赖解析尊重 `rust-version` 下限。
2. 特性统一语义保持 resolver v2 的既定语义不变，因此依赖可选 provider 的
   `--features test-support` 门禁口径与形态矩阵不受影响。
3. MSRV 感知解析只改变**候选版本**的挑选顺序，不改变依赖图结构；`Cargo.lock`
   仍是发布的依赖事实源，锁定模式门禁照常生效。

## 影响

- `cargo update` 之后 `Cargo.lock` 可能位移，位移方向是"MSRV 兼容的更旧版本"，
  不是"更旧但不兼容的版本"。
- 锁文件位移必须逐条 diff 复核，确认没有意外降级或依赖图增删；未复核的位移不得
  进入发布。
- 依赖若确实需要高于 1.99.0 的 MSRV，只能通过提升本仓库 floor 并同步
  `rust-toolchain.toml` 解决，不允许靠放宽解析器绕过。

## 验证

- `cargo metadata --locked` 通过，锁文件与清单一致。
- 解析政策对照实验：同一份清单下，resolver 2 与 resolver 3 的 `cargo update --dry-run`
  提案完全一致——当前依赖集在两种政策下解析结果相同，升级不改变既有依赖版本；它改变的是
  **未来**挑选候选版本时的默认偏好。
- 锁文件位移的归因纪律：位移必须逐条 diff，并归因到具体来源（清单侧的依赖需求更新，或
  解析政策变化）。无法归因的位移不得进入发布。
- 一次受控 `cargo update` 的 diff 只包含 MSRV 感知选择导致的版本变化，不含依赖图
  增删。
- 全部平台构建与测试在 1.99.0 工具链下保持绿色，即所选依赖版本确实与 floor 兼容。
