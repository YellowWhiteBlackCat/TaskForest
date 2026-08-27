# TaskForest 质量门禁

本文定义当前公开仓库的验证层级。门禁证明行为、边界和发布构造，不使用内部评分、截图数量
或主机回执替代产品验证。

## 1. 合并门

对 `main` 的 pull request 和 push 同时运行 Linux 与 Windows 阻塞 CI：

| 检查 | 目的 |
|---|---|
| public-repo guard | 拒绝私有路径、真实截图、个人邮箱、凭据模式和主机路径 |
| cargo-deny | 拒绝已知漏洞、不允许的许可证和依赖策略违规 |
| Python/Shell 政策门 | 检查安装清单、自动化安全、模块边界和测试布局 |
| rustfmt / clippy | 格式和 warning-free 编译 |
| release build | 验证默认 GPUI 发布形态 |
| package stage simulation | 验证 Linux 安装树、权限和 polkit 路径 |
| workspace nextest | 行为和平台无关契约 |
| doctests / rustdoc | 文档代码和公开 API 链接 |
| fallback feature matrix | 验证可选 provider 不产生产品 SKU 分叉 |

CI 使用锁文件和固定 Rust 工具链，Cargo 并行度不超过四。Windows 原生边界在每次 PR 与
main push 的 portability workflow 中阻塞运行；macOS 保持手动/月度 advisory。跨平台编译
不能替代原生 API 证据。

## 2. 本地层级

统一入口为：

```bash
bash scripts/quality/local-gates.sh quick
bash scripts/quality/local-gates.sh standard
bash scripts/quality/local-gates.sh extended
```

- `quick`：公开边界、文档、格式、模块、安装清单、自动化和测试布局政策门；
- `standard`：quick + dependency audit、clippy、nextest、doctest、rustdoc、release build 和
  平台无关形态矩阵；
- `extended`：standard + coverage、mutation、Miri、fuzz 和性能/体积回归。

Windows 开发机使用 `scripts/windows/local-gates.sh`，通过 Git Bash 调用同一组可移植门禁；
Windows telemetry、测试和 helper 不使用 PowerShell 或其他命令解释器采集系统事实。

## 3. 公开仓库门禁

`scripts/quality/public_repo_guard.py` 检查 Git 跟踪内容：

- 禁止 `.private/`、`docs/archive/`、内部评分/TODO/路线图和 host receipt；
- `docs/screenshots/` 只允许公开策略 README；
- 禁止常见私钥、token 和云凭据格式；
- 禁止未允许的邮箱以及真实用户 home/media 路径；
- 文本解码失败或可疑的大型系统快照必须显式处理，不能静默跳过。

正式发布额外使用 history 模式，要求所有历史作者邮箱使用 GitHub noreply，并确认历史对象
中不再包含私有路径。仓库首次公开前必须完成一次历史重写并通过该模式。

## 4. 文档门禁

公开文档遵循 [docs/README.md](README.md) 的路由：

- 每篇 living 文档不超过 200 行，AGENTS 保持短小；
- 本地链接必须存在且不能逃出仓库；
- 每个 crate 必须有 Role、Boundary、Contract and verification；
- 当前文档不能链接历史、评分、TODO 或真实回执；
- 规则变化直接重写正文，不追加日期流水。

## 5. 发布物门禁

tag 发布必须同时构建并验证：

- `taskforest_<ver>_amd64.deb` 与 `taskforest_<ver>_arm64.deb`：`dpkg-deb --info` 和
  `--contents`；
- `taskforest-<ver>-1.x86_64.rpm` 与 `taskforest-<ver>-1.aarch64.rpm`：`rpm -qp --info`、
  `--list` 和 `--requires`；
- `TaskForest-<ver>-x64.msi` 与 `TaskForest-<ver>-arm64.msi`：WiX 构建、MSI 数据库反编译和
  payload 引用检查；
- 每个架构独立的 Linux/Windows SHA-256 清单。

Windows 或 Linux 任一打包 job 失败，整个发布失败。所有必需文件验证完成后，单一 publish
job 才能创建或更新 GitHub Release，避免半成品发布。

## 6. 截图与真实环境

真实主机截图和回执只保存在忽略的 `.private/` 或 `target/` 中。公开截图必须来自确定性
演示数据，并检查用户名、进程、网络、设备、路径和元数据。无真实目标环境时只能报告
SKIP，不能把 fixture、编译或静态图片写成平台验证通过。

## 7. 验证器质量

验证器必须自动发现范围、执行目标、检查结果和副作用，并在范围为空、解析失败或回执不完整
时 fail-closed。禁止用源码字符串存在性、恒真断言、固定测试数量或 `echo PASS` 证明行为。
