# TaskForest 的 crate 结构

TaskForest 是一个 Rust workspace：它把「从操作系统采集事实 → 领域规则 → 应用编排 → 平台
I/O → 前端渲染」拆成一条单向依赖链。这样拆的目的很实际——平台差异、渲染技术、权限边界互不
污染，任何一处换实现都不牵动整条链。本页是 crate 层的项目地图：说明这条链分成哪几层、每层在
解决什么，并给出全部 crate 的 README 索引。

## 依赖方向

```text
frontend → application → core / shell → platform runtime → app-host / native composition → OS
```

返回方向只携带类型化的事件、快照和失败。一条事实只有一个权威来源；下层可以展开上层，不能重新
定义上层。

## 各层在解决什么

- **core / application / shell**：所有前端共享的领域事实、命令与投影。`core` 拥有类型化事实与
  纯规则，`application` 拥有命令、reducer 与端口，`shell` 拥有前端中立的投影、缓存与交互词汇。
- **contract**：`taskmanager-ui-contract` 与 `taskforest-contract` 规定前端和平台
  「必须提供什么」，本身不含实现。
- **platform**：provider SPI 与各平台适配器负责真实 I/O，runtime 负责调度、并发、背压与事件
  投递，app-host 负责挑选并组合平台实现。
- **frontend**：GPUI、Iced、TUI、Bevy 是四个独立产品 crate，共享同一套应用投影，区别只在渲染
  与交互；共享的 CLI harness 提供它们共同的启动骨架。
- **受审边界与工具**：只有少数受审 crate 允许 `unsafe` 或直接触碰内核/系统接口，能力按需授权、
  可单独撤销；`test-support` 与 fuzz workspace 只服务测试，不属于产品依赖。

## Crate 索引

每个 crate 的职责、边界与合同见它自己的 `README.md`（Role / Boundary / Module map /
Contract and verification）。超出 README 的专题细节下沉到该 crate 的 `docs/` 专题章程，由
README 的 `Docs` 段引用；`docs/`（仓库级）只保留跨 crate 的共性章程。crate 的完整分母由
Cargo metadata 决定，下表是按层归组的阅读地图：

**领域与应用编排**

- [taskforest-core](taskforest-core/README.md) — 类型化领域事实、可用性语义与纯规则
- [taskmanager-application](taskmanager-application/README.md) — 命令、reducer、端口与能力请求
- [taskmanager-shell](taskmanager-shell/README.md) — 前端中立状态机与共享投影

**契约与 SPI**

- [taskmanager-ui-contract](taskmanager-ui-contract/README.md) — 前端必须提供的 UI 语义契约
- [taskforest-contract](taskforest-contract/README.md) — 平台能力、请求、事件与端口契约
- [taskmanager-platform-provider](taskmanager-platform-provider/README.md) — provider SPI 与注册
- [taskforest-ipc](taskforest-ipc/README.md) — 守护进程跨进程二进制通信协议与传输通道

**执行运行时**

- [taskmanager-platform-runtime](taskmanager-platform-runtime/README.md) — 有界执行、公平投递与 ECS 调度内核
- [taskforest-daemon](taskforest-daemon/README.md) — 后台系统守护进程服务与时序持久化单写者

**平台适配器与 OS 边界**

- [taskmanager-platform-linux](taskmanager-platform-linux/README.md) — Linux `/proc`、`/sys` 采集适配
- [taskmanager-platform-windows](taskmanager-platform-windows/README.md) — Windows 适配组合
- [taskmanager-platform-macos](taskmanager-platform-macos/README.md) — macOS 适配组合
- [taskmanager-platform-android](taskmanager-platform-android/README.md) — Android provider seam（feature 门控）
- [taskmanager-platform-ohos](taskmanager-platform-ohos/README.md) — OpenHarmony adapter seam
- [taskmanager-platform-portable](taskmanager-platform-portable/README.md) — 跨平台复用的安全 I/O provider
- [taskmanager-platform-conformance](taskmanager-platform-conformance/README.md) — 平台一致性场景
- [taskmanager-windows-api](taskmanager-windows-api/README.md) — 受审 Windows API 边界
- [taskmanager-accessibility-linux](taskmanager-accessibility-linux/README.md) — Linux AT-SPI 语义桥

**宿主组合与启动**

- [taskmanager-app-host](taskmanager-app-host/README.md) — 工具箱中立的本地组合根
- [taskmanager-platform-native](taskmanager-platform-native/README.md) — 编译期选择原生适配器
- [taskmanager-cli](taskmanager-cli/README.md) — 四前端共享的 CLI 启动骨架

**前端产品**

- [taskmanager-gpui](taskmanager-gpui/README.md) — GPUI 桌面前端（当前发布表面）
- [taskmanager-iced](taskmanager-iced/README.md) — Iced 桌面前端
- [taskmanager-tui](taskmanager-tui/README.md) — 终端前端
- [taskmanager-bevy-ui](taskmanager-bevy-ui/README.md) — Bevy 第四前端

**UI 组件与资产**

- [taskmanager-ui](taskmanager-ui/README.md) — GPUI 组件层（ADR-017）
- [taskmanager-theme](taskmanager-theme/README.md) — 工具箱中立主题 token
- [taskmanager-icons](taskmanager-icons/README.md) — 语义图标注册表
- [taskmanager-assets](taskmanager-assets/README.md) — 产品身份资产

**受审边界与 helper**

- [taskmanager-escalation](taskmanager-escalation/README.md) — 按特性提权 seam 与非特权默认策略
- [taskmanager-afpacket](taskmanager-afpacket/README.md) — 受审 AF_PACKET 边界
- [taskmanager-fd-bridge](taskmanager-fd-bridge/README.md) — 受审 SCM_RIGHTS 描述符桥
- [taskmanager-perf-ioctl](taskmanager-perf-ioctl/README.md) — 受审 `perf_event_open` 边界
- [taskmanager-msr-helper](taskmanager-msr-helper/README.md) — CPU MSR 读取 helper
- [taskmanager-rapl-helper](taskmanager-rapl-helper/README.md) — Intel RAPL 功率 helper
- [taskmanager-privilege-helper](taskmanager-privilege-helper/README.md) — fixed Intel GPU PMU and SMART self-test helpers
- [taskmanager-process-control-helper](taskmanager-process-control-helper/README.md) — 身份校验的受控进程操作 helper
- [taskmanager-setup-helper](taskmanager-setup-helper/README.md) — 首次运行 RAPL/udev 设置 helper
- [taskmanager-net-launcher](taskmanager-net-launcher/README.md) — CAP_NET_RAW 启动 helper
- [taskmanager-smbios-helper](taskmanager-smbios-helper/README.md) — SMBIOS 读取 helper
- [taskmanager-smbios-tables](taskmanager-smbios-tables/README.md) — SMBIOS 解析纯库
- [taskmanager-tray-muda](taskmanager-tray-muda/README.md) — Windows/macOS 托盘桥

**遥测存储**

- [taskmanager-telemetry-store](taskmanager-telemetry-store/README.md) — 有界内存遥测历史与读模型
- [taskmanager-history-store](taskmanager-history-store/README.md) — 持久 JSONL 遥测历史

**测试支持**

- [taskmanager-test-support](taskmanager-test-support/README.md) — 跨 crate 测试夹具（仅 dev 依赖）

## 怎么找到某个 crate

- 按任务查 [docs/README.md](../docs/README.md) 的任务路由表，它会指向受影响的 crate README。
- 上层导航：顶层 [AGENTS.md](../AGENTS.md) 管总，本页承接 crate 层地图，各 crate README
  再展开自己的 `docs/` 专题章程。
