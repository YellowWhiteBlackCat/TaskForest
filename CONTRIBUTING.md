# 贡献指南

感谢关注 TaskForest。本文说明如何报告问题、提交变更，以及仓库的机械质量约束。
仓库使命与不变量见 [AGENTS.md](AGENTS.md)，文档路由从
[docs/README.md](docs/README.md) 开始。

## 报告问题

提交 issue 请使用对应模板，并尽量填写运行环境：操作系统与版本、桌面环境、
Wayland/X11、安装方式。TaskForest 是跨平台系统监视器，环境差异是最常见的根因。

- 安全漏洞不要使用公开 issue，见 [SECURITY.md](SECURITY.md)；
- 不要附带真实主机截图或系统快照，公开截图政策见
  [docs/screenshots/README.md](docs/screenshots/README.md)。

## 开发环境

- 工具链由 `rust-toolchain.toml` 锁定为 stable 最新版；
- Cargo 验证一律带锁文件（`--locked`），并行度不超过四；
- 测试一律使用 `cargo nextest ... -j 4`（doctest 仅可使用
  `cargo test --doc ... -j 4`）；quick 门禁会机械拒绝其他形式。

## 本地门禁

提交前至少运行：

```bash
bash scripts/quality/local-gates.sh quick
```

涉及行为或发布物时运行 `standard`；`extended` 覆盖 coverage、mutation、Miri 和
fuzz，用于深度改动。Windows 使用 `scripts/windows/local-gates.sh`。层级定义见
[docs/QUALITY_GATES.md](docs/QUALITY_GATES.md)。

## 提交与分支

- 提交信息使用 Conventional Commits；scope 用领域或 crate 名（如 `gpui`、`iced`、
  `capture`、`packaging`、`ci`、`docs`），不要用版本号；
- 一个提交或 PR 只做一件事；大改动拆成能独立通过 quick 门禁的小批次；
- 当前为单人主干模式：routine 工作可直接在 `main` 推进，但推进前必须通过 quick
  门禁，push 后 CI 必须为绿；发布稳定期的收尾工作在 `release/X.Y` 分支进行并经
  PR 并回 `main`，生命周期见 [docs/RELEASE.md](docs/RELEASE.md)；
- tag 只由发布流程产生，一般贡献不需要打 tag。

## 仓库硬约束（CI 机械拒绝）

- 业务 crate 一律 safe Rust；`unsafe` 仅存在于已审计的边界 crate；
- Windows telemetry、测试与 helper 不使用 PowerShell 或其他命令解释器；
- 不新增 `foo/mod.rs`，使用 `foo.rs` + `foo/` 模块形态；
- 生产代码 panic-free；测试证明行为与副作用，不做源文本断言或宿主特定断言；
- 公开历史不接受私有路径、真实截图、个人邮箱、凭据模式和宿主绝对路径。

## CI 与配额

- 对 `main` 的 PR 与 push 运行 Linux 与 Windows 阻塞检查（portability 另含 macOS
  advisory）；GitHub Actions 配额有限，请避免无意义重跑；
- Dependabot 版本更新因配额关闭，依赖升级以定期 `cargo update` chore PR 进行；
  安全告警保持开启。

## 文档

行为或契约变化必须同步更新对应文档；`docs/` 总纲每篇不超过 200 行，写作规则见
[docs/README.md](docs/README.md)；不可逆决策新增 [ADR](adr/)。
