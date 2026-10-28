# 跨前端 UI 场景平权审计总纲

前端平权的判定标准是能力与场景的 1:1 全射。GPUI 作为基准发布表面，其拥有的全部 55 个专用交互场景均属于产品必须作答的业务契约。任何一端缺失对应入口或像素实证，都属于未平权状态：owner 已登记的存异标 `AcceptedDifference`，场景子句被明确拒绝的标 `Partial` / `Unsupported`，其余标 `Pending`。

本表只判定 **场景级语义**：入口、状态机、行为测试、截图矩阵与当前截图。组件级差异（Tooltip、TextInput、ModalOverlay 等）由 `crates/taskmanager-ui-contract/src/capabilities.rs` 与四端 `src/capabilities.rs` 的声明拥有；产品意图级差异由四端 `src/functional.rs` 的 CORE-04 声明拥有。本表只在场景主路径受这些差异影响时引用 owner 声明，不复制第二套词汇。

## 状态词汇

| 状态 | 判定 |
|---|---|
| `Aligned` | 入口、状态机、行为测试、截图矩阵与当前截图均已验证，场景语义与 GPUI 等价。 |
| `AcceptedDifference` | 场景语义已交付，但主表面/模态按 owner 声明有意不同；必须带差异索引。 |
| `Partial` | 场景已交付一部分，至少一个定义内子句被 owner 明确拒绝；必须带拒绝声明。 |
| `Unsupported` | 该端有意不提供该场景；必须带 owner 理由。 |
| `Pending` | 尚未达到 `Aligned`，也没有可引用的 owner 声明；不能计入平权完成。 |

`Aligned` 不允许用作“看起来差不多”的同义词。一个场景只有同时具备正常入口、状态机、可发现的行为测试、当前截图矩阵行四项，才允许写 `Aligned`；缺一项即为 `Partial` / `Pending`，owner 已明确拒绝时用 `AcceptedDifference` / `Unsupported`。`Unsupported` / `Pending` 场景不要求截图矩阵行；`Aligned` / `AcceptedDifference` / `Partial` 场景必须保留可执行截图行。

## 场景全景审计与当前状态

| 分组 | GPUI 场景 Token | 场景定义 | Iced | Bevy | TUI | 差异/证据 |
|---|---|---|:---:|:---:|:---:|---|
| 基础与系统 | `about` | 应用版本、许可证与仓库说明 | Aligned | Aligned | Aligned | — |
| | `system-about` | 独立操作系统与桌面环境完整事实面 | Aligned | Aligned | Aligned | — |
| | `first-run` | 首次运行系统引导配置 | Aligned | Aligned | Aligned | — |
| | `system-dashboard` | 多硬件总览仪表盘 | Aligned | Aligned | Aligned | — |
| | `system-hardware` | SMBIOS 硬件拓扑明细 | Aligned | Aligned | Aligned | — |
| | `system-npu` | NPU 神经网络遥测明细 | Aligned | Aligned | Aligned | — |
| 进程诊断切片 | `process-selection` | 选中单行进程基准态 | Aligned | Aligned | Aligned | — |
| | `process-properties-performance` | 进程性能指标明细 Tab | Aligned | Aligned | Aligned | — |
| | `process-memory-pss-swap` | 进程 PSS/USS/Swap 内存展开 | Aligned | Aligned | Aligned | — |
| | `process-network-details` | 进程网络 Socket 与 RTT 明细 | Aligned | Aligned | Aligned | — |
| | `process-gpu-details` | 进程 GPU 显存与引擎明细 | Aligned | Aligned | Aligned | — |
| | `process-resource-limits` | cgroups/prlimit 资源配额明细 | Aligned | Aligned | Aligned | — |
| | `process-isolation` | 命名空间沙箱与 Seccomp 审计 | Aligned | Aligned | Aligned | — |
| 危险操作确认 | `process-force-kill` | 强制终止（SIGKILL）警示弹窗 | Aligned | Aligned | Aligned | — |
| | `process-tree-confirm` | 结束完整进程树连带确认框 | Aligned | Aligned | Aligned | — |
| | `process-batch-confirm` | 批量终止多个选中进程确认框 | Aligned | Aligned | Aligned | — |
| | `smart-self-test-confirm` | 磁盘 SMART 自检二次确认框 | Aligned | Aligned | Aligned | — |
| | `users-session-confirmation` | 用户会话断开/锁定的冻结目标确认框 | Aligned | Aligned | Aligned | — |
| 存储与硬件诊断 | `storage-health` | 文件系统健康、只读与错误状态及 SMART 自检报告 | Aligned | Aligned | Aligned | — |
| | `smart-missing-tool` | 缺失 smartctl 时的引导提示 | Aligned | Aligned | Aligned | — |
| | `smart-permission` | SMART 特权提权失败/引导态 | Aligned | Aligned | Aligned | — |
| | `partition-disk-usage` | 分区挂载点与磁盘空间图 | Aligned | Aligned | Aligned | — |
| | `partition-live-usage` | 磁盘动态吞吐与 IOPS 视图 | Aligned | Aligned | Aligned | — |
| | `device-hotplug` | 设备热插拔事件触发与重排 | Aligned | Aligned | Aligned | — |
| | `sensor-center` | 温度、风扇与功率读数及不可用原因 | Aligned | Aligned | Aligned | — |
| | `battery-fan-performance` | 电池状态与风扇转速联动卡 | Aligned | Aligned | Aligned | — |
| | `battery-live-performance` | 电池实时放电速率与电压曲线 | Aligned | Aligned | Aligned | — |
| | `gpu-engine-inventory` | 多 GPU 引擎枚举与负载图表 | Aligned | Aligned | Aligned | — |
| | `intel-gpu-telemetry` | Intel 专属硬件遥测扩展指标 | Aligned | Aligned | Aligned | — |
| 服务与系统诊断 | `service-details-logs` | 服务实时日志流输出面板 | Aligned | Aligned | Aligned | — |
| | `services-search-highlight` | 服务名关键词搜索过滤态 | Aligned | Aligned | Aligned | — |
| | `diagnostic-preview` | 系统一键诊断报告生成后预览 | AcceptedDifference[D] | Aligned | AcceptedDifference[D] | [D] |
| | `diagnostic-failure` | 诊断生成失败或超时错误模态 | Aligned | Aligned | Aligned | — |
| 开机启动项分析 | `startup-impact` | 启动项引导耗时影响等级分析 | Aligned | Aligned | Aligned | — |
| | `startup-failure-evidence` | 启动服务崩溃/异常退出取证 | Aligned | Aligned | Aligned | — |
| | `startup-boot-markers` | systemd-analyze 开机关键链时间轴 | Aligned | Aligned | Aligned | — |
| 监控、告警与事件 | `active-alert` | 顶部活动告警横幅展开态 | Aligned | Aligned | Aligned | — |
| | `alert-rules-manager` | 自定义告警规则配置中心 | Aligned | Aligned | Partial[A] | [A] |
| | `event-center` | 系统安全与审计事件追踪中心 | Aligned | Aligned | Aligned | — |
| | `saved-view-presets` | 保存、应用、删除、JSON 导入导出与跨会话恢复 | Aligned | Aligned | Aligned | — |
| | `telemetry-paused` | 遥测全局暂停冻结状态水印 | Aligned | Aligned | Aligned | — |
| 历史数据回放 | `history-replay` | 性能指标持久化历史窗口与趋势图 | Aligned | Aligned | Aligned | — |
| | `history-60m` | 系统仪表盘 60 分钟全局历史趋势图 | Aligned | Aligned | Aligned | — |
| | `application-history-replay` | 应用身份 CPU 趋势与内存/进程数峰值回溯 | Aligned | Aligned | Aligned | — |
| 交互与显示形态 | `apps-search-highlight` | 进程搜索关键字高亮与命中过滤 | Aligned | Aligned | Aligned | — |
| | `apps-group-expanded` | 进程分组/进程树全部节点展开 | Aligned | Aligned | Aligned | — |
| | `apps-identity-matrix` | 进程身份校验与伪装报警状态 | Aligned | Aligned | Aligned | — |
| | `apps-zero-gray` | 进程零值置灰与活动值对比渲染 | Aligned | Aligned | Aligned | — |
| | `settings-switch-focus` | 设置页焦点切换与高亮状态 | Aligned | Aligned | AcceptedDifference[F] | [F] |
| | `settings-zero-gray` | 设置项置灰与可用性状态对比 | Aligned | Aligned | Aligned | — |
| | `settings-permission-center` | 特权 Helper 授权管理中心视图 | Aligned | Aligned | Aligned | — |
| | `keyboard-focus` | 全键盘无障碍导航焦点环形态 | Aligned | Aligned | AcceptedDifference[F] | [F] |
| | `vertical-nav` | 垂直导航栏折叠与图标态 | Aligned | Aligned | AcceptedDifference[V] | [V] |
| | `sidebar-hidden` | 性能页左侧设备导航栏收起态 | Aligned | Aligned | Unsupported[S] | [S] |
| | `sidebar-edit` | 性能页设备显示/隐藏列编辑态 | Aligned | Aligned | AcceptedDifference[E] | [E] |

差异索引：

- `[A]` TUI 的 `AlertRuleAuthoring` / `AlertRuleTransfer` 为 `Unsupported`：终端只支持查看与切换托管规则，不支持自定义规则编写和 JSON 导入导出（`crates/taskmanager-tui/src/functional.rs:32,35`；`crates/taskmanager-tui/docs/SURFACES_AND_STATE.md:28`）。
- `[S]` TUI 没有桌面侧栏，使用资源选择器代替；`ToggleSidebar` 是 deliberately unbound（`crates/taskmanager-tui/src/bindings.rs:25`；`crates/taskmanager-tui/src/ui/help.rs:33`）。
- `[F]` TUI 的 `FocusVisible` 为 `Divergent`：cursor row 是唯一焦点指示，没有 pointer/keyboard modality ring（`crates/taskmanager-tui/src/capabilities.rs`）。
- `[V]` TUI 的 vertical nav 是 header-line 模式，不是带图标的垂直导轨（`crates/taskmanager-tui/src/ui/header.rs:44`）。
- `[E]` TUI 的 sidebar edit 是模态编辑器，不是内联列编辑（`crates/taskmanager-tui/src/ui/sidebar_editor.rs`）。
- `[D]` `DiagnosticBundle` 为 `AcceptedDifference`：Iced 提供 clipboard report，TUI 提供 plain-text report，GPUI 提供 preview/write bundle（四端 `src/functional.rs`）。

## 组件与意图级存异

本表不重复登记组件级差异；权威在以下声明：

- 组件能力：`crates/taskmanager-ui-contract/src/capabilities.rs` 与四端 `src/capabilities.rs` 声明 19 个非 `Ported` 单元格（Iced 4、TUI 9、Bevy 6），其中 1 个为 `Unsupported`（TUI Tooltip）。
- 产品意图：四端 `src/functional.rs` 声明 9 个非 `Reference` 意图（TUI 2 个 `Unsupported` + 7 个 `AcceptedDifference`）。
- 特性账本：`crates/taskmanager-ui-contract/src/feature_coverage.rs` 注册 107 个 `FeatureId`，其中 106 个各采纳一个 225 蓝图槽位（含 10 个 Wave 3/4 交付与 29 个新推进特性：LTTB 下采样、饱和度分级、D-state 占比、原生告警集成、效率模式、复合过滤、套接字状态跟踪、文件系统挂载审计、no-new-privs、四项服务诊断项、八项硬件/遥测项，以及八项控制/取证/驱动项：冻结解冻控制、ZSWAP/内存压缩、固件安全启动、GPU 节流原因、GPU 驱动固件 API、进程用户凭据、服务状态机与服务 cgroup 内存限额），另 1 个是额外的 `memory.process-swap-charge`（不占蓝图槽位）；`pressure.memory-thrashing-health` 额外作为 slot 179 的声明别名；该注册表里的 23 个特性在四端一致声明 `Unsupported`。
- 蓝图处置：完整 225 槽产品 census 由 `scripts/parity/blueprint_dispositions.tsv` + `BlueprintDispositionTable` 拥有（ADR-059）：107 个 `Adopted(FeatureId)`、118 个显式 `Unsupported`（附公开原因）、0 个未处置；未采纳槽位由 126 项收敛至 118 项。`Unsupported` 不是 `Ready`，不补证据锚点；采纳某项时新增 `FeatureId` 并在同一变更中把槽位翻为 `Adopted`。
- 特性证据：`scripts/parity/feature_evidence.tsv` 的 212 个特性格已全部锚定（0 pending、0 dangling）；原先 3 个 pending 特性（多格式导出、多分辨率环、内存抖动健康分）已由四端真实行为测试关闭，Bevy 也补齐了带扣分明细的健康事实行。

因此 `Aligned` 只表示场景语义等价，不表示像素或手势一致；像素级一致仍须由 capture 与 capability 声明共同判定。

## 平权推进与收敛机制

1. **绝对基准**：本表 55 个场景 Token 属于硬性全集，任何针对 GPUI 场景的扩充必须同步在此登记。
2. **状态推进**：每一端新增场景时，必须同时满足代码状态机接线、单测断言、场景矩阵收录、真实 Wayland 截图生成四项，才允许将状态升级为 `Aligned`；`AcceptedDifference` 必须引用 owner 声明，`Partial` / `Unsupported` 必须引用拒绝理由，`Pending` 不计入完成。
3. **闭环守护**：`scripts/quality/visual_capture_coverage.py` 校验本表 Token、状态词汇、runner 覆盖与截图矩阵行（`Unsupported`/`Pending` 不要求截图行）；`scripts/parity/scene_registry.tsv` 为 55 个场景各绑定四端证据（当前 219 个行为测试锚点，TUI `sidebar-hidden` 为显式缺席），`scene_registry_guard.py` 对 live nextest discovery 与截图矩阵校验集合、顺序与悬空锚点；`parity-evidence` 再校验四端交互矩阵 197 个锚点与 8/8 需求覆盖。逐场景行为锚点的语义贴合仍由 owner 审查，门禁负责存在性、归属与悬空校验。
