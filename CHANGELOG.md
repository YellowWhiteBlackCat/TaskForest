# 更新日志

本文件记录面向用户的版本化变化：能力新增、缺陷修复、平台与打包状态变化。纯工程
内部重构在不影响安装产物或使用方式时不记录。

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)；0.x 阶段的
次版本号可以承载破坏性变更。发布与 tag 规则见 [docs/RELEASE.md](docs/RELEASE.md)。

## [Unreleased]

## [0.1.0-rc7] — 2026-09-01

### 变更

- 前端架构改为四个独立 product（ADR-051）：GPUI（`taskforest-g`）、Iced
  （`taskforest-i`）、TUI（`taskmanager-tui`）、Bevy（`taskforest-b`）各自是独立的
  crate 加二进制，共享统一的 CLI（`--json`、`--suggest-thresholds`、
  `--gpu-engines`、`--memory-smbios`、`--package-power`、`--msr`、`--help`），能力差异
  （如 TUI 的 `--snapshot`、Windows GPUI 的 `--capture-window`）按产品如实呈现。源码
  构建从 `cargo build --release` 改为 `cargo build --release -p taskmanager-gpui`；
  工作区不再有 `ui-*` feature，`cfg` 只用于平台轴。主题与图标层不再包含任何 toolkit
  代码，前端绑定（`taskmanager-ui::theme_binding`、`taskmanager-iced::theme_binding`）
  由各前端自行拥有。
- 真实截图测试改为每次运行独占 UUID、runtime、D-Bus、KWin 配置/数据/缓存、应用二进制、
  用户 cgroup 和 receipt；supervisor/watchdog 在父进程异常退出后回收完整进程树，GPUI/TUI
  的 latest 通过锁与原子 pointer 发布。
- RC7 完成四前端当前构建的后台 Niri/KWin 证据链和 A/B 交叉隔离验证；捕获流程不创建
  主机桌面图标、不改变主机 Wayland/D-Bus/KWin 状态。

### 新增

- 设置页新增"窗口边框"策略（Linux/Wayland）：跟随系统（默认，保持合成器协商）、
  系统标题栏（显式请求原生装饰）、应用标题栏（请求 CSD 自绘标题栏与透明圆角，含
  应用内最小化/最大化/关闭按钮）。切换经 gpui `Window::request_decorations` 实时
  生效；合成器拒绝请求时（如 GNOME/Mutter 无法绘制原生标题栏）以 toast 诚实告知
  实际生效的模式，绝不静默丢弃用户选择。渲染始终跟随合成器实际授予的装饰事实，
  任何情况下不会出现双标题栏。
- GPUI Performance 页面统一采用单一的 frame/content budget：CPU、内存、磁盘、网络、GPU、
  电池和风扇页面共享有界主视图与 stats rail，低空间按完整内容组降级，不以滚动或裁切伪装完整。
- 设置页新增集中可选硬件权限中心，统一展示授权中、拒绝、不可用、不支持和失败等 typed 状态；
  授权入口保持按能力单发，不自动弹窗或轮询。
- Linux/Wayland GPUI 新增一次性当前窗口 PNG 捕获：由 KDE Spectacle 固定参数提供，app-host
  在后台校验并原子发布；Portal Screenshot v3 与 ScreenCast/PipeWire 保留为后续演化路径。

### 修复

- 修复纵向导航 rail 中新增截图入口后控制按钮越界的问题；窄 rail 使用紧凑图标目标，完整标签通过 tooltip 提供。

## [0.1.0-rc6] — 2026-08-31

### 新增

- SMBIOS 内存明细提权链：新增 `--memory-smbios` CLI 表面与请求 lane，经
  pkexec 专用只读 helper（`taskforest-smbios-helper`）读取 root-only 的
  type-17 记录（插槽占用、实时配置频率、颗粒详情），缺数据保持 typed 不可用，
  绝不伪造；免特权侧的 DMI 探针改与 helper 共用同一纯解析器
  （`taskmanager-smbios-tables`），两读者永不漂移。同一 helper 与 lane 现已
  覆盖 type 0/1/2 身份表（系统/主板序列号、产品 UUID、资产标签、SKU——这些
  `/sys/class/dmi/id` 节点为 root-only），系统页硬件区按需授权后展示。
- CPU 页与系统页新增授权面板：性能→CPU 详情的"封装功耗"区与系统页的
  "内存清单"卡片，均由对应请求 lane 供数；数据不可得时给出类型化原因，
  需提权时提供单发"授权"按钮（一次点击恰触发一次请求，不自动轮询、不自动
  弹授权框）。
- CPU 封装功耗提权链：新增 `--package-power` CLI 表面与请求 lane，经
  `taskforest-rapl-helper` 对 root-only RAPL energy 计数做一次定窗采样并给出
  每封装瓦数；两条链均配套 polkit action、包安装清单与安装器事务
  （`smbios` / `rapl`）。
- Intel MSR 读数族（ADR-048）：新增 `--msr` CLI 表面与
  `taskforest-msr-helper`（对 root-only `/dev/cpu/*/msr` 做纯 pread 文件读，
  不新增 unsafe 信任根），给出封装温度、当前/最小/最大倍频与核心电压；
  CPU 详情页新增 MSR 读数区（单发授权按钮，与封装功耗同一交互纪律）。
  基频 BCLK 采用 CPUID leaf 0x16 总线参考频率（内核 tsc.c 同源判定；sysfs
  base_frequency ÷ 效率比与 TSC 计时 ÷ 比两条路径经实测证明产出错误数值，
  已在 ADR-048 记录否决）。AMD Zen 1–3 倍频/电压经 RDMSR P-state 寄存器
  实装（ADR-049）；SMN 遥测需写副作用/MMIO，安全 Rust 下结构性不可达，
  AMD 温度继续走免特权 k10temp 路径。
- Windows UAC 提权通道 stage 2（ADR-035）：外进程控制的越权路径改经
  `ShellExecuteExW("runas")` + 一次性回执文件（调用落在审计边界
  windows-api 内，业务 crate 保持全安全 Rust），取消/缺装/超时/协议损坏
  各自类型化；macOS 授权词表与纯分类层就绪，Security 框架调用以待签名
  helper 的打包 ADR 为界如实保持未接。
- Linux GPU 驱动版本扩展：模块自报版本的 DRM 驱动与 NVIDIA procfs
  `NVRM` 版本进入"驱动版本"行；Mesa 用户态版本因不存在免 GL-loader 的
  稳定数据源保持类型化缺失（结论已记录）。
- GPU 驱动版本成为 core 类型化事实：Linux NVML 与 Windows 适配器版本号贯通
  四前端"驱动版本"行；Windows 侧旧有"版本冒充驱动名"的交叉接线移除。
- 芯片组型号：Linux 经 PCI 主桥/PCH ISA 桥 + pci.ids（hwdata）解析出芯片组
  营销名，系统页三前端展示；pci.ids 查询提升为共享模块，GPU 路径不再私持。

### 修复

- 修正 `--help` 中 `--gpu-engines` 描述被后续条目截断的排版；未知参数的用法
  提示补上 `--snapshot` 与 `--capture-window`。

### 工程与流程

- 为 0.1.0 正式版建立对外流程基线：贡献指南、安全上报策略、issue 与 PR 模板；
- 明确分支与发布线生命周期、预发布准出条件（见 docs/RELEASE.md）；
- 权限模型 Boundary 3 补记 SMBIOS/RAPL 两个既有提权 feature 的文档缺口。

## [0.1.0-rc5] — 2026-08-28

### 修复

- 保持重要指标页布局完整，并收敛一处 Windows API 调用边界；
- 移除托盘依赖中带已知漏洞的 glib feature 路径。

### 变更

- Iced 前端的"性能"页面对齐 GPUI 布局权威；
- 采集测试与真实桌面完全隔离，避免嵌套采集影响宿主环境。

> 自 rc5 起预发布后缀统一连写（`rcN`，无点）。内部曾组装 rc4 构建但未打 tag
> 发布，其内容已全部并入 rc5；该编号不回收。

## [0.1.0-rc.3] — 2026-08-27

### 修复

- 清除全部依赖安全告警；
- 修复 Windows MSI 安装器的 WiX UI 扩展安装问题。

## [0.1.0-rc.2] — 2026-08-27

### 修复

- Linux 与 Windows 发布打包问题。

### 变更

- Windows 控制台行为与安装路径界面调整；
- CPU 首次采样不再显示误导性占用。

## [0.1.0-rc.1] — 2026-08-27

### 新增

- 首次公开预发布：GPUI 桌面前端，以及 Linux（DEB/RPM）与 Windows（MSI）自动打包
  发布流水线。
