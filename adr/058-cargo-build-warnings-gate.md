# ADR-058: 警告门槛迁移到 Cargo build.warnings

Status: accepted

- 相关：`.cargo/config.toml`、`scripts/quality/local-gates.sh`、
  [QUALITY_GATES.md](../docs/QUALITY_GATES.md)

## 背景

`.cargo/config.toml` 此前用 `rustflags = ["-D", "warnings"]` 把警告提升为错误。
`rustflags` 是编译单元指纹的一部分：任何一次改动都会让本地产物整体失效重建。在
多前端并行、共享 `target/` 的工作区里，这等于把"改一行配置"放大成"全量重编译"。

Cargo 1.97 稳定了 `build.warnings`（取值 `warn` / `allow` / `deny`），作用对象是
**本地包**（工作区成员），正是本仓库需要的作用域：第三方依赖的警告仍由依赖自己
负责。本仓库工具链固定 1.99.1，该键可用。

## 决策

1. `.cargo/config.toml` 改用 `[build] warnings = "deny"`，删除 `rustflags`。
2. CI 保留 `RUSTFLAGS` 注入：它承载 mold 的 `-C link-arg=-fuse-ld=mold`，且 CI 与
   本地门禁脚本都会显式导出 `-D warnings`。因此两端严格度一致，`build.warnings`
   不构成降级。
3. 警告门槛的语义是"本仓库包零警告"；第三方依赖的警告不因此变成错误，也不被本门禁
   静默。

## 影响

- 本地普通 `cargo build` / `cargo check` 不再因 flags 变动而整体重建缓存。
- 门槛位置从"编译命令参数"变为"工作区构建配置"，与 ADR-056 的 lint 事实源同属
  工作区级配置；二者叠加后本仓库包在本地与 CI 都是 warning-free。
- `RUSTFLAGS` 在 CI 仍然生效，mold 链接参数不丢失；本地若要使用 mold，必须自行在
  `RUSTFLAGS` 中声明，不得因此放宽警告门槛。

## 验证

- 本地 `cargo build` / `cargo check` 在无源码改动时不再重建产物。
- 引入一个真实 warning 的最小改动在本地下必须编译失败，在 CI 同样失败。
- CI 的链接参数仍包含 mold 的 `-C link-arg`，链接产物与切换前一致。
