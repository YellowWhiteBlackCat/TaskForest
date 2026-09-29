# TaskForest · 任务森林

> **护眼原生系统监视器** —— 面向进程、服务、设备与系统健康。

TaskForest 是一个用 **Rust 2024** 构建的跨平台系统监视器，运行在 **Linux、Windows 和
macOS** 上。它想回答一个很朴素的问题：当我们长时间盯着一台机器时，怎样既能看清它，
又不让眼睛和情绪被它拖垮。

## 名字里的森林

一台正在运行的机器，从来不是一行行等待清点的数字，而更像一片此起彼伏的森林：有新生的
进程，有休眠的服务，有早已枯死却仍占着内存的旧枝，偶尔也有失控蔓延的东西。监视器的意义
不是替你去"清理"，而是让你在长久注视之后，依然看得清这片林子真实的样子——谁在生长，
谁在沉睡，哪里是真的安静，哪里只是没被看见。

**任务森林**取的正是这层意思：不打扰，但看得清。

## 为什么是"护眼"

护眼不只是把界面调暗。TaskForest 有一套名为 **EyeForest（护眼森林）** 的低眩光、低饱和
绿色主题，专为长时间注视而设计：它克制刺目的红，不靠闪烁和弹窗制造焦虑，让异常以清楚而
安静的方式被看见。

同时，界面支持亮色、暗色、高对比度、键盘导航、英中双语和无障碍语义——舒服地看，也应该
让所有人都能看。

## 我们如何看待数据

这是 TaskForest 与"又一个任务管理器"最不一样的地方：

- **诚实优先于好看。** 未知、真实的零、暂时失败、权限不足和平台不支持，是五种不同的
  东西。读不到就如实说读不到，TaskForest 不会用空列表或 `0` 把失败伪装成平静。
- **克制优先于全能。** 主程序始终以普通用户身份运行；需要更高权限的能力单独申请、单独
  撤销，每一步都可见。
- **同一语义优先于表面一致。** 平台能力可以不同，但同一个概念在三平台、四前端上的含义
  必须相同：标签、缺失性和后果一致，像素和手势可以不同。做不到的能力以明确的"不支持"
  呈现，而不是一个点了却没有反应的按钮。

这些原则不是口号，而是仓库的硬约束。想了解它们如何落到分层、权限与验证上，见
[AGENTS.md](AGENTS.md) 与[当前架构](docs/ARCH.md)。

## 主要能力

- CPU、内存、磁盘、网络、GPU、电池、传感器和硬件信息；
- 应用与进程树、搜索、排序、列配置、批量操作和详情检查；
- 服务、启动项、登录会话和容器视图；
- 亮色、暗色、EyeForest 护眼主题，键盘导航、英中双语和无障碍语义；
- JSON 快照、类型化失败状态，以及按功能授权的可选高级采集。

不同系统和硬件上的可用字段并不完全相同。缺少数据源或权限时，界面会显示明确的不可用
原因，而不是留白或归零。

## 前端

四个前端是四个独立产品，共享同一组领域事实、命令和状态投影，各自拥有独立的安装包：

| 前端 | 产品 | 二进制 | 定位 |
|---|---|---|---|
| GPUI | TaskForestG | `taskforest-g` | GPU 加速桌面，当前主桌面表面（Wayland-only） |
| Iced | TaskForestI | `taskforest-i` | 响应式桌面（Wayland-only） |
| Ratatui | TaskForest | `taskforest-t` | 终端前端，不依赖显示服务器 |
| Bevy | TaskForestB | `taskforest-b` | 实验性 ECS 开发前端（Wayland-only） |

## 平台状态

| 平台 | 当前状态 |
|---|---|
| Linux / Wayland | 主要开发平台，覆盖最完整 |
| Windows | 原生边界和核心功能可用，仍在扩大真机验证 |
| macOS | 架构和平台适配已建立，真机验证与打包尚未完成 |

平台能力以类型化结果呈现：要么是可用值，要么是明确的不可用原因。平台按真实原因诚实降级是
产品承诺的一部分，因此"四端平权"指发行物与前端语义同权，并不意味着三个平台具备相同的
系统级能力。

## 构建与安装

每个前端是独立的 product crate（ADR-051），可以单独构建：

```bash
cargo build --locked --release -p taskmanager-gpui    # target/release/taskforest-g
cargo build --locked --release -p taskmanager-iced    # target/release/taskforest-i
cargo build --locked --release -p taskmanager-tui     # target/release/taskforest-t
cargo build --locked --release -p taskmanager-bevy-ui # target/release/taskforest-b
```

只有推送与根 `Cargo.toml` 版本一致的 `vX.Y.Z` 或 `vX.Y.Z-rcN` tag，才会创建正式 Release
并生成官方产物：

- Linux `.deb`：`amd64`、`arm64`；`.rpm`：`x86_64`、`aarch64`；四端各一套；
- Windows `.msi`：`x64`、`arm64`；四端各一套；
- 共享数据包 `taskforest-common` 与四端安装包配合分发。

已发布的 Linux 与 Windows 安装包可在 GitHub Releases 获取。macOS 打包、签名和公证暂缓。
四个前端共享同一套 core/application/shell/theme 下层栈，平台差异是唯一的条件编译轴；
Rust 版本、平台依赖、包内容和发布规则见[文档中心](docs/README.md)与
[发布说明](docs/RELEASE.md)。

> [!WARNING]
> TaskForest 仍处于实验阶段，尚无稳定版本，当前属于早期 0.x 正式发行面，版本线为
> `0.2.0-rc2`。部分平台数据源、安装流程、签名和真实硬件验证仍在持续完善，请勿把它作为
> 生产环境的唯一监控或管理工具。

## 当前限制

- 尚未完成所有平台和硬件组合的真实数据验证；
- Windows 安装包在配置 Authenticode 证书前为未签名包；
- macOS 暂无正式安装包、签名和公证流程；
- 部分 SMART、GPU、风扇、节流、网络归因和无障碍场景仍依赖目标设备验证；
- 公开仓库不提交真实主机截图、系统快照、测试回执、内部评分、路线图或 TODO。

产品截图将在确定性演示数据和隐私检查流程完成后发布；当前仓库刻意不展示真实设备采集画面。

## 文档

产品定位和成熟度从本页了解，动手改代码前请按任务查[文档中心](docs/README.md)的权威路由：

- [当前架构](docs/ARCH.md) · [状态所有权](docs/STATE_OWNERSHIP.md)
- [权限与信任边界](docs/PERMISSION_MODEL.md) · [质量门禁](docs/QUALITY_GATES.md)
- [发布与打包](docs/RELEASE.md) · [产品身份](docs/PRODUCT_IDENTITY.md)
- [贡献指南](CONTRIBUTING.md) · [安全策略](SECURITY.md) · [更新日志](CHANGELOG.md)
- [开源致谢](docs/ACKNOWLEDGMENTS.md) · [各 crate 的职责](crates/README.md)

## License

TaskForest 使用 Apache-2.0 许可证，详见 [LICENSE](LICENSE)。
