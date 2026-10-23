# 更新日志

本文件记录面向用户的版本化变化：能力新增、缺陷修复、平台与打包状态变化。纯工程
内部重构在不影响安装产物或使用方式时不记录。

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)；0.x 阶段的
次版本号可以承载破坏性变更。发布与 tag 规则见 [docs/RELEASE.md](docs/RELEASE.md)。

## [0.4.0] — 2026-10-22

> 本次为**破坏性次版本**：0.x 阶段的次版本号承载破坏性变更。本版本对已发布 crate 的
> 公共 API 做了硬切换（`taskmanager-ui-contract` 的 `IconId` 新增语义变体并移除
> `taskmanager-ui` 中重复的高亮匹配引擎、合同层新增带类型的容器配额 / 启动阶段 /
> NPU 功耗 / 进程批量冻结等事实字段），下游 crate 需按新 API 更新。

### 新增

- **Core 遥测：NPU 功耗与能耗事实**：新增 `NpuPowerReport` / `NpuPowerObservation`
  类型化功耗（瓦特）与累计能耗（焦耳）观测；平台层仅在驱动暴露真实功耗或能量计数器时
  填充，无计数器的加速器不贡献观测值，能耗序列永远不从利用率曲线推导。
- **Core 遥测：SMART 工具探测命令与修复引导**：新增 `SMART_TOOL_PROBE_COMMAND`、
  `SmartToolGuidance`、`SmartToolRemediation` 类型，将磁盘健康工具的探测与修复建议
  升级为结构化事实。
- **Core 遥测：启动阶段时序快照**：新增 `StartupPhase`、`StartupPhaseKind`、
  `StartupTimelineSnapshot` 类型，将启动过程从无差别的时间线段升级为有阶段语义的
  结构化快照。
- **Core 进程：批量确认冻结附带集**：新增 `FrozenProcessCollateral` /
  `FrozenProcessCollateralMember`，进程树批量操作确认界面所画的层级列表与实际执行集
  来自同一次展开，冻结后不再回读活表。
- **Core 遥测：进程资源配额与通用采样点**：新增 `ProcessTelemetry::Quota` 资源配额
  类型与 `MetricSample` 通用采样点。
- **Bevy 前端（B）全页面模块深化**：Alerts 页（横幅弹簧、暂停覆盖层、严重性标签、
  导出 Toast）、Performance 页（磁盘指标、NPU 功耗面板、悬停图表、侧边栏逐核列表）、
  进程树页（徽章、折叠箭头、附带集层级、引导线、悬停、挂载点、规划）、进程属性弹窗
  （背板、GPU 标签页、内存标签页、配额标签页、标签页骨架）、Services 页（动作字形、
  依赖树、日志跟踪、日志文本、状态点、单元过滤）、Sessions 页（气泡、类型标签）、
  Settings 页（行组件）、Startup 页（取证面板、影响评估、墨色方案、阶段轴、时间线）、
  System 页（总览卡片、隐私面板、拓扑视图）全部落地为独立子模块。
- **Bevy 前端（B）官方控件接入**：滑动开关（`Switch`）接入官方 `bevy_ui_widgets`
  Checkbox 行为原件并发布 `Switch` 无障碍角色，能力状态由 Divergent 升格为 Ported；
  告警规则编辑器接入官方 `bevy_ui_widgets` Slider 用于阈值滑杆；页面滚动条接入官方
  `bevy_ui_widgets` Scrollbar，半透明浮动轨道配胶囊拇指。
- **Bevy 前端（B）场景连接与缓动**：新增 `scene_attach` 场景挂载协议与 `easing`
  缓动曲线控件。
- **Iced 前端（I）性能图表动态与光栅化**：新增 `perf_chart/motion` 动画过渡与
  `perf_chart/raster` 光栅化渲染路径。
- **Iced 前端（I）磁盘吞吐量轴阻尼过渡**：磁盘吞吐量图的 Y 轴上限跟踪流量峰值，
  切换时经 120 ms 阻尼过渡（轴与曲线同步移动），并新增挂载点标签槽位。
- **Iced 前端（I）网络连接语义胶囊**：Insights 面板的网络连接行从纯文本升级为语义
  胶囊（`pills::connection_row`），附带 `chips` 标签组件。
- **Iced 前端（I）应用列表交互增强**：新增 `emphasis` 行高亮与 `sort_tint` 排序
  色调标记。
- **Iced 前端（I）虚拟列表窗口化**：新增 `virtual_list/window` 视口窗口化模块，
  文本行几何量测接缝（`text_metrics` 行几何）就位。
- **UI 合同：列宽常量公开**：`COLUMN_WIDTH_KEYBOARD_STEP`、`COLUMN_WIDTH_MAX`、
  `COLUMN_WIDTH_MIN` 从 `columns` 模块导出，`columns` 升级为 `pub` 模块。
- **图标资产扩充**：`taskmanager-assets` 新增 RGBA 图标集与矢量图标补充。

### 变更

- **文档同步更新**：ARCH、QUALITY_GATES、RELEASE、STANDARDS、STATE_OWNERSHIP、
  SYSTEM_INSTALL_MANIFEST、UI_COMPONENT_ARCHITECTURE 同步至当前代码状态。
- **质量门禁精简**：移除不再适用的 `nightly-quality.sh` 与
  `perf-ioctl-privileged-coverage.sh`，更新 Bevy 场景棘轮台账。

### 修复

- **Bevy 前端（B）系统页总览网格换行**：总览网格此前按**边框盒**宽度解析列数，而卡片
  实际只占**内容盒**宽度（右侧 8px 滚动条 lane 属于边框盒），第 4 张卡片因此被挤到
  第二行；网格现在按内容盒宽度解析，2×2 与单行两种排布都成立。
- **Bevy 前端（B）页面内容重复挂载**：路由资源变更后，旧页面内容与新内容可能同帧并存
  （两套 `PageContent`）。页面生命周期改由**路由资源本身**裁定，挂载系统保证任意时刻
  恰好一个页面内容，旧内容先隐藏、帧末再卸载，官方控件观察器不会拿到已消失的实体。
- **Bevy 前端（B）存储热插拔滑动与电池充电流**：设备分类容器在窄布局下不可见时，列表项
  测得 0 高，滑动动效永远等待测量；动效宿主改为按当前可见的磁盘分类推进，进度以类型化
  数值逐帧前进。
- **Bevy 前端（B）会话气泡卡被共享模态压掉**：气泡卡只在共享确认模态未挂载时显示，避免
  两个确认面同时存在；会话页自身的控制按钮按产品路径武装会话控制门，气泡卡随之出现。
- **搜索匹配引擎单一权威**：`taskmanager-ui` 的高亮匹配组件与 `taskmanager_core::core::text`
  的匹配引擎重复实现同一语义，现已删除前端副本，匹配范围只由核心层产出。

## [0.3.2] — 2026-10-20

### 修复

- **Bevy 前端（B）的「界面尺寸」档位现在完整缩放类型阶梯**：控件/行高、图标尺寸与字号
  全部走 rem 轴，改 Small/Standard/Large 时表面按档位整体重算（此前图标尺寸写死为
  像素值，不随档位变化）；rem 尺寸轴同时由 `bevy_rem_guard.py` 门禁机械强制。

### 变更

- **Bevy 场景律法升级为机械门禁**：`bevy_bsn_guard.py`（唯一 `bsn!` 构造路线）、
  `bevy_rem_guard.py`（rem 尺寸轴：零 `FontSize::Px`、px 派生值不作布局长度、图标
  适配器不收裸数字尺寸）与 `bevy_scene_gate.py`（共享包 `zhuge-ui-kit` 随 crate 发布的
  唯一执行器，禁硬编码非零 px 字面量，存量走棘轮台账）注册进 quick 门禁与 CI。

## [0.3.1] — 2026-10-18

### 新增

- **Linux 现代沙箱 Flatpak 分发清单与构建自动化**：
  - 编写官方 Flatpak 构建清单（`packaging/flatpak/io.github.YellowWhiteBlackCat.TaskForest.json`），配置 Wayland、`/proc:ro`、`/sys:ro` 只读硬件挂载及 IPC 运行时通信通道；
  - 提供单文件 bundle 构建脚本（`build-flatpak.sh`）及质量门禁守卫（`test_flatpak_manifest.py`）。
- **Linux 容器与虚拟化进程树折叠与标签视图**：
  - 支持 cgroups v1 / v2 容器签名解析，精准识别 Docker、Podman、CRI-O、containerd、Flatpak、Snap 容器沙箱内进程；
  - 在四端前端（GPUI、Iced、TUI、Bevy）进程详情页完整渲染醒目的容器专属 Badge（如 `[Docker: a1b2c3d4e5f6]`、`[Flatpak: app.id]`）。
- **长效运行内存防泄漏与句柄稳定性浸泡压测线束**：
  - 增加长驻留浸泡稳定性检测守卫（`soak_stability_guard.py`），验证连续高频数据采样与文件轮转下物理驻留内存（RSS）收敛及文件描述符（FD）零泄漏；
  - 快速质量门禁阶段扩展至 56 stages 全部打通。
- **Crates.io 独立底层库发布预备态达成**：
  - 补全 `taskmanager-core`、`taskmanager-ipc`、`taskmanager-daemon`、`taskmanager-platform-contract` 的发布级元数据、文档与独立 LICENSE 副本，独立沙箱打包与校验通过。

## [0.3.0] — 2026-10-17

### 新增

- **系统守护进程 (`taskforest-daemon`) 与后台时序持久化**：
  - 正式落地 ADR-055 架构决策，支持 7×24 小时后台连续遥测采集与时序滚动持久化，彻底突破进程退出即丢失历史记录的痛点。
  - 拥有唯一的后台历史持久化单写锁，前端只作为非特权观察客户端连接，实现特权面与复杂 GUI 渲染栈解耦。
  - 编写系统级与用户会话级 systemd 服务单元（`packaging/daemon/`），并打包为独立的 `TaskForest-Daemon` DEB 与 RPM 安装包。
- **高性能跨进程二进制通信协议 (`taskmanager-ipc`)**：
  - 基于 20 字节固定帧头（Magic `TFIP`、版本 1、Sequence、CRC32 校验码与 16 MiB 熔断保护），保障跨进程通信的极端安全与防腐。
  - 抽象跨平台传输通道：Linux/macOS 走 Unix Domain Socket，Windows 映射 Named Pipe。
  - 引入强类型按需采样位掩码 `SubscriptionMask`，服务端动态聚合多前端订阅集，无活跃连接时自动休眠降频。
- **四端双模态连接引擎 (`AppHostMode`)**：
  - 前端在 `taskmanager-app-host` 中构建智能自适应探测，优先连接本地守护进程订阅实时流；在无服务、便携运行或容器环境下自动透明保底降级为 Standalone 单进程直采模式。
  - 构建 `DaemonSubscriptionWorker` 后台流式拉取通道，统一驱动四端（GPUI、Iced、TUI、Bevy）实时指标消费。
- **官方品牌矢量徽标全链路接入与 XDG 逆向域名规范对齐**：
  - 将官方绿色松树仪表盘矢量徽标收录至 `taskmanager-assets`，提供权威公开访问接口。
  - GPUI 标题栏与侧边栏接入官方品牌 Logo 渲染；TUI 终端标题栏接入常青松树品牌徽标 `🌲 TaskForest`。
  - 修正 Linux 打包安装路径，同时安装全套尺寸的逆向域名 hicolor 阶梯图标（`io.github.YellowWhiteBlackCat.TaskForest`），彻底消除 GNOME/KDE 任务栏与 Dock 显示空白默认齿轮的问题。
- **crates.io 独立底层库发布标准与跨平台铁律**：
  - `taskmanager-core` 就地升格，补齐发布元数据、Doctest、开源 Headless 范例（<50 行），固化零 I/O、零时钟纯粹性并保证确定性。
  - 全网实时查重锁定 `taskforest-*` 独占命名矩阵，并在 `docs/STANDARDS.md` 第 7 节与 `AGENTS.md` 确立平台无关律、三平台对等适配律、平台小尾巴特化律与 SemVer 兼容性门禁。

## [0.2.3] — 2026-10-16

### 变更

- **CI 工作流现代化升级**：全量 GitHub Actions 官方 Action 升级至原生 Node 24 稳定版本（`upload-artifact@v7`, `download-artifact@v8`, `action-gh-release@v3`），消除 Node 20 弃用警告。
- **sccache 分布式编译缓存**：跨 Linux、Windows、macOS 流水线引入 `sccache` 与 GitHub Actions 存储桶后端，配合既有 `rust-cache` 建立对象级细粒度编译缓存体系。
- **Windows CI 矩阵解耦并行化**：将 Windows 单 runner 串行检查拆分为独立的并行矩阵任务，大幅缩短 PR 原生安全回执与跨前端构建验证的耗时。
- **本地质量门禁体验优化**：在 `local-gates.sh` 的 `quick` 模式下智能识别未构建的提权 helper 并优雅提示跳过，不再硬性打断开发者本地快速质量检查。
- **图形与 EGL 探针增强**：`test_capture_isolation.py` 引入轻量级虚拟合成与 EGL 设备探针，在无 GPU 直通或受限容器环境下快速降级，杜绝 240 秒超时挂死。

## [0.2.2] — 2026-10-13

### 安全

- 更新 `rustls` 至 `0.23.45`，修复 [GHSA-2mjx-qc3c-rqvc](https://github.com/advisories/GHSA-2mjx-qc3c-rqvc) 中的 TLS 1.3 握手消息加密级别校验漏洞。

### 变更

- 更新全部 Cargo 锁文件中受现有 SemVer 与 Rust 1.99 工具链约束的依赖至最新稳定兼容版本。
- GPUI 后端迁至 GPUI Kit `0.7.3` 锁定的 GPUI `0.3.9`；关闭 Kit 自带组件和资产层，继续使用 TaskForest 的组件与主题。
- 启用 GPUI 支持的平台默认、次像素和灰度文字渲染模式，并持久化用户选择。

## [0.2.1] — 2026-10-12

### 新增

- **SMART 自检能力与授权**：按自检类型呈现设备明确报告的支持状态；Linux 通过固定操作 helper 与 polkit 一次性授权执行，GPUI、Iced、TUI、Bevy 共用同一能力投影。
- **Applications 聚合指标状态**：四端应用组行展示部分 CPU/内存总值与覆盖数，详情和辅助技术标签保留完整状态及失败原因。
- **四端 session 控制确认**：Disconnect/Lock 操作显示冻结的目标身份与操作摘要，目标已替换、缺失或重复时拒绝提交。
- **Bevy 前端界面尺寸档位**：设置页新增 Small/Standard/Large 三档界面尺寸
  （正文 14/16/18px），与 GPUI/Iced 共享同一配置权威并跨会话恢复；Standard 档
  正文与说明文字对齐 16px/12px 契约（原 14px/10px）。
- **Bevy 前端页签键盘导航**：导航条与进程属性弹窗页签迁移到官方 headless
  `TabList`——方向键漫游焦点、Enter/Space 选择、共享 tablist/tab 无障碍角色。

### 修复

- **身份替换保护**：进程、登录会话、磁盘与告警行使用稳定生命周期身份；刷新后目标已替换时，旧选择和旧确认不会转向新对象。
- **遥测缺失值呈现**：CPU 当前值缺失或非有限时按缺失状态呈现，不再显示 `NaN%`；独立可用的频率事实继续显示。

### 变更

- **Bevy 升级到 0.20.0 正式版**（自 `0.20.0-rc.2`，源码零改动），同步落地 0.20
  能力：`RemSize`/`rem()` 尺寸层、`Ready` 挂载完成事件、`FixedNode` 视口锚定
  浮层（确认面、悬浮提示、菜单）、`InlineBox`/`InlineImage` 行内图文
  （表头排序指示与列标签同线排布）。

## [0.2.0] — 2026-10-06

### 新增

- **保存视图与导航同权**：四端提供保存、应用、删除及 JSON 导入/导出，保存结果跨会话恢复；
  四端支持纵向导航及设备侧栏的排序、显示与隐藏。

- **四端进程属性**：冻结进程身份的概览、四项历史趋势、命令和七类按需诊断切片；
  刷新、授权和完整列表访问经共享应用投影执行，进程退出或 PID 复用不会重定向查看目标。

- **TUI 终端诊断报告导出**：经共享诊断引擎生成脱敏文本报告，支持指定路径落盘与状态反馈，
  失败时如实告警，不再写出未脱敏内容。
- **`taskforest-common` 共享数据包**：Linux 共享图标资产改由公共数据包独占承载，
  GPUI/Iced/Bevy 图形包依赖它，消除多包共装同一路径的 dpkg/rpm 归属冲突；
  发布产物矩阵扩展为 28 项构建产物（新增 `TaskForest-Common` 的 DEB 与 RPM，含
  amd64/arm64），另附 4 份 sha256 校验清单，发布附件共 32 个。
- **诚实的能力状态词汇**：新增能力级 `CapabilityStatus::RequiresEscalation`，
  "可提权" 与 "需授权/硬拒绝" 不再折叠为同一状态，四端据此呈现可操作的一次性授权入口。

### 修复

- **紧凑窗口与完整操作**：修正 Bevy 进程表底部裁切、Iced 长标签和分组计数重叠、
  TUI 帮助滚动遗漏及 GPUI 嵌套树展开；Bevy/TUI 事件中心支持完整历史查看、导出与清空。

- **进程树确认**：冻结完整子树并保留叶节点先于根节点的提交顺序，确认摘要对应所选根进程。
- **历史与可用性**：修复 Iced 打开性能页丢失历史峰值及字节单位读数、TUI 内存概览漏掉 Swap、
  GPUI 七类诊断状态折叠及晚到结果丢失、Bevy 启动时确认弹窗丢失。

- **Windows MSI 跨产品误删**：修正卸载某一前端时误删其他前端开始菜单标记与目录的问题
  （共享组件改用引用计数，快捷方式保持各自组件）。

### 变更

- **四端正式发行物**：GPUI、Iced、TUI、Bevy 均提供 Linux x64/arm64 的 DEB/RPM
  及 Windows x64/arm64 的 MSI；macOS 安装包、签名与公证暂缓。

- **Bevy 首启设置**：把此前声明却并不存在的首启入口改为诚实的能力缺失。
- **公开口径**："四端平权" 明确限定为四端发行物与前端语义同权，不含 "跨平台功能对等"；
  平台缺席一律以 typed 结果诚实呈现。
- **TUI 静态化与打包基座升级**：`taskforest-t` 改为 musl 静态链接（零运行时库依赖，
  DEB/RPM 移除 libc6/glibc 依赖声明）；Linux 包构建基座升至 ubuntu-26.04，GUI 包的
  glibc 与显示栈依赖底线随之抬高。

## [0.2.0-rc3] — 2026-10-01

`0.2.0` 最终版的第三个预发布候选。

### 新增

- **全端导出与快照格式对齐**：Bevy 前端补齐结构化多格式快照导出能力（TSV、CSV、HTML、JSON），支持快捷键与应用层快照会话集成。
- **深度硬件遥测全端接线**：
  - CPU 拓扑缓存（L1d、L1i、L2、L3）、大小核异构核心分类与 RAPL 封装功耗完整接入 Bevy。
  - 每核心时钟频率与核心网格时钟接入 Iced 与 Bevy。
  - 被删除但仍持有的句柄泄漏检测与句柄类型分类接入 Bevy 与 Iced。
- **TUI 静态 musl 二进制分发**：在 Ubuntu 26.04 环境下引入 TUI 静态 musl 构建支持，降低终端版本的动态链接库依赖。

### 平台与门禁

- **四端能力基线门禁收敛**：`PLATFORM_GATE_BASELINE` 实现四端声明对齐，四端当前均严格以 typed Unsupported 呈现 25 项全端待攻坚能力，基线缺失数统一收敛至 75。

## [0.2.0-rc2] — 2026-09-29

`0.2.0` 最终版的第二个预发布候选。

### 新增

- **PROCHOT 热节流实时呈现**：经特权 MSR 读路（IA32_THERM_STATUS）读取 PROCHOT
  状态位，四端 System 页同步呈现热节流源与每核心状态；数据缺席时如实标注，
  不伪造就绪。

### 修复

- **Bevy 偏好跨会话保留**：外观（模式/对比度）、语言、刷新节奏与历史容量此前
  重启即丢，现经共享配置协调器在启动组合恢复；设置页回显按主题模式身份诚实
  读回，不支持的取值不再伪造选中态。
- **GPUI Apps 页排序**：行排序跟随实际可见列，而非固定列序。
- **Bevy 能力诚实**：撤回后端无法支撑的剪贴板入口，不再呈现不可用的功能。

## [0.2.0-rc1] — 2026-09-23

首个携带 `taskforest-common` 共享数据包拆分的发布候选：Linux 共享图标资产改由该
数据包独占承载，接管下界固定为首个拆包版本 `0.2.0-rc1`（以格式自身排序 `~` 进入
DEB/RPM 版本比较）。本次候选包含的变化现已汇入 `0.2.0` 正式版。

## [0.1.4] — 2026-09-05

### 新增

- **四端同构（Four-UI Parity）阶段性补强**：统一 `PROCESS_COLUMNS`，补齐多项进程、
  硬件、电源和诊断投影，并把 GPUI、Iced、TUI、Bevy UI 的 headless 测试执行器统一到
  `nextest`。本版本不把尚未完成的真实平台采集、像素级截图矩阵或交互闭环标记为已验收；
  各项能力仍以当前门禁和私有逐项验收记录为准。

## [0.1.3] — 2026-09-04

### 新增

- **四端全量功能与视觉平价基线（Milestone 3）**：
  - **Bevy UI**：补齐容器管理页（Containers）、CPU 亲和性设置（Affinity）、服务详情弹窗与依赖拓扑面板（Dependencies Panel）、系统托盘集成（System Tray），完善选中进程详情与全套无头行为断言。
  - **Iced UI**：集成 Linux AccessKit 原生无障碍桥接，支持完整的屏幕阅读器语义树投影与动作分发；补齐 F5/F9/Alt+1..8/Esc/Ctrl+F/Ctrl+Space 全局快捷键与 Alerts 路由；性能页 CPU 视图接入 RAPL 功耗与 MSR 实时读数。
  - **TUI**：补齐 CPU 亲和性编辑、服务日志导出、健康与告警浮层路由、以及与 AccessKit 契约对齐的终端语义快照。
  - **GPUI**：进程详情面板新增环境变量键值预览卡片（Environment Card）；完善高对比度与跟随系统主题切换；强化 AccessKit 动作响应。
- **Linux 多端原生 DEB 打包**：
  - 扩展 Linux 打包流水线，在既有 GPUI 发布物基础上，原生支持构建与验证 Iced (`TaskForest-I`)、TUI (`TaskForest-T`)、Bevy (`TaskForest-B`) 三个独立前端形态的完整 DEB 安装包，严格遵循统一安装目录与 Wayland-only 规范。

## [0.1.2] — 2026-09-03

### 修复

- **Windows 编译与跨平台窗口捕获修复**：修复在 Windows 目标下编译 GPUI 时因缺少 frame capture 导出符号导致的构建阻断（为 `patches/gpui/src/platform.rs` 补齐非 Linux/macOS 平台上的 fallback 桩实现，并在 `gpui.rs` 中无条件导出）；完善跨平台活动窗口捕获体系，在 `taskmanager-platform-windows` 中基于 `windows-capture` 提供原生窗口截图，在 `taskmanager-platform-native` 中建立统一的 In-Process → Native → Unsupported 三层回退流水线。
- **发布流水线恢复**：修复 0.1.1 触发的 Windows MSI CI 构建阻断，恢复多平台完整自动化发布。

> 注：0.1.1 tag 在发布流水线中遭遇 Windows x64 MSI 编译阻断未生成正式 GitHub Release 资产；0.1.1 所含全部变更并入 0.1.2 正式发布。

## [0.1.1] — 2026-09-03

### 修复

- **内存暴涨与 Vulkan 描述符池防护**：通过本地 vendored patch 替换 `blade-graphics 0.7.1` 的描述符分配器，废除原本的 `16.pow(k)` 几何级数暴涨策略（曾扩容至 4096-set 导致 Intel 驱动单次映射 ~50 MiB、双 command buffer 内存飙升至近 200 MiB）；采用单池 256-set 封顶与单 buffer 64 池硬上限，并在每次帧重置时销毁附池、重建基准池以确保显存与匿名页映射在空闲期彻底回落；清理上游 7 处生命周期省略警告。
- **服务、启动、用户页面空数据修复**：修复页面切换导航分发中遗漏的数据采样分支，进入 Services、Startup、Users 页面时分别按需发起专属刷新请求，并补齐完整的端到端自动化测试验证“切换页面 → 发出请求 → 接收快照 → 界面数据生成”。
- **趋势文本与键值面板对齐重构**：在基础 UI 库的 `KeyValueRow` 中增加 `new_multiline` 构造支持，建立标准双栏多行排版：左侧标题锁定首行靠左，右侧多行数值垂直堆叠右对齐，行高根据内容自然撑开，彻底杜绝单行长文本挤压导致的文本覆盖重叠、截断或 `...` 缩写缺陷；性能页图表角标增加 `flex_wrap` 弹性约束，防止次级与紧凑图表越界。
- **Wayland 纯进程内窗口自截屏**：打通 GPUI `BladeRenderer` 到 Vulkan 交换链帧回读管道，开启 `TextureUsage::TARGET | TextureUsage::COPY` 并在绘制完成后通过 Staging Buffer 直接将 GPU 显存像素读回 CPU，使用内置 `image` crate 原生编码 PNG，平台层建立优先级直连；彻底废除通过子进程调用外部 KDE `spectacle` 的做法，消除桌面环境依赖并根除失焦截错其他窗口的问题。
- **演示模式与文案规范**：GPUI 现在真正支持 `taskforest-g --demo`，使用确定性演示数据启动完整桌面界面，不读取主机遥测、不写配置或历史，也不执行主机控制操作；配置、浏览器、服务日志和搜索反馈补齐中英文文案；首次运行入口改为 TaskForest 文档，不再指向上游 Mission Center Wiki。

## [0.1.0] — 2026-09-02

### 修复

- Windows MSI 现在随 GPUI 安装同目录的身份校验 UAC process-control helper；缺失或损坏安装仍以 typed `HelperUnavailable` 失败，不伪造授权成功。
- 服务日志一次性快照现在携带并校验请求 ID；刷新、重开或切换服务后，迟到的旧快照不会覆盖当前日志状态。Iced 服务详情保持流式请求单一路径。
- 当前目录诊断包与快照导出现在只接受单一的跨平台文件名组件；路径穿越、设备名、分隔符和大小写别名在应用层拒绝，主机运行时再次闭合边界。诊断脱敏同时覆盖 Unix/Windows UNC 路径。
- GPUI 的图表场景、历史 sample、滑动时序、hover 节流和键盘路由不再使用进程级静态/线程局部状态，统一由每个 RootView 持有；无障碍动作现在按已发布语义 revision 校验后进入共享选择与 surface owner。
- GPUI 图表摘要改为固定在卡片左下安全带，和画布右上角的双向读写值徽标分离；720×760 紧凑窗口的双向图也由无头几何合同覆盖，避免摘要与徽标重叠。
- Iced 与 GPUI 的桌面宿主生命周期统一收口：二次启动只激活已有窗口，托盘可用时关闭窗口进入最小化、托盘退出才终止进程；Iced 恢复路径明确解除最小化并请求窗口焦点，macOS 单例释放不再等待连接。
- 四端求同存异矩阵继续收口：Iced 文本选择的 Ctrl-A/多行行为改为显式差异，Bevy 的 process-details 摘要级能力与缺口进入完整 ledger，Bevy 也纳入本地化 key、来源 manifest 和共享边界门禁扫描。
- Bevy 选中进程详情现在拥有明确的 wheel-only 滚动边界，低窗口中不会把概览/Insights 下半段静默裁掉；无可见滚动条仍作为产品差异保留并标注。

## [0.1.0-rc9] — 2026-09-01

### 修复

- 统一系统遥测的 current/partial/stale/unavailable 折叠；过期数据保留最后可信事实，未观测数据继续显示不可用，不再由前端各自推导。
- 统一进程树选择、批量控制、优先级和库存控制的 typed 语义；前端只提交原子请求，目标身份与确认冻结由 shell/application 负责。
- 将 Saved View 剪贴板协议收归 core 单一权威，GPUI 与 Iced 共用格式、校验、命名冲突和 ID 分配规则。
- 移除 Iced 的装饰性“以管理员身份运行”入口；真正的按功能提权仍需独立的 OS-native capability/helper 通道，不再伪装成已支持。
- 移除 Bevy 与 Iced 已被现行实现取代的旧骨架、死代码、重复事件载荷和交互矩阵残留；Bevy 页面、确认面和可用性门禁继续保持无 panic。
- 将 Iced 外观、动画策略和输入焦点可见性收回实例状态，避免并行窗口/测试之间通过进程级可变静态互相污染。

### RC8 遗留状态

- RC8 的 CI checkout、Windows 条件编译、macOS portability 边界和 Linux 多架构 helper 清单修复继续保留，并由本轮全 workspace 门禁复验。
- RC8 之后审计发现的装饰性提权、诊断导出 JSON 往返、批量操作文案、Bevy fold 边界和 Iced 外观隔离问题已在本 RC 收口；Bevy 菜单的键盘优先差异仍按前端能力声明，不伪称为统一 pointer 菜单。

## [0.1.0-rc8] — 2026-09-01

### 修复

- 修复干净 CI checkout 下诊断包、窗口捕获和快照导出测试的仓库 scratch 目录初始化。
- 修复 Windows 非 Linux 构建中误编译 current-window capture 状态机和 Linux-only 测试的问题。
- 修复 macOS portability 测试范围与主线程限定的原生菜单测试边界。
- 修复 Linux 多架构打包遗漏 SMBIOS、RAPL 和 MSR helper 的问题；RC8 Rehearsal 已验证
  amd64/arm64 DEB、RPM 以及 Windows x64/arm64 MSI 和 checksum 清单。

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

> rc6 曾在发布分支上组装并完成部分验证，但在正式打 tag 前被后续 rc7 的四前端
> 架构收口取代，因此没有 `v0.1.0-rc6` tag；编号不回收，内容已由 rc7/rc8 延续。

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
