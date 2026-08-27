# 027: Renderer-independent frontend shell state

Status: accepted（shell 轨供 TUI/Iced 消费；GPUI 保留直连视图轨）

## Context

application 与 `ui-contract` 已提供 renderer-neutral 的命令、reducer、port、失败与
typed 快照，但前端状态（页选择、进程选择/搜索/排序、模态优先级、滚动告警历史、键
分发）最初在 TUI 内自持一份，会让后续前端各自复制行为。GPUI 的 `RootView` 则内嵌
`!Send` Entity（focus/IME/LayerStack/输入实体）且要求多窗口 per-window 状态，完整
`ShellApp` 迁移与 gpui 的状态模型不兼容。

## Decision

建立 `taskmanager-shell` 作为 renderer-independent 前端状态层，并成文**双轨契约
（求同存异）**。`SystemProjectionStore`（`PlatformEventBatch` → typed
`BatchFoldOutput`）是该层唯一的**数据折叠**：三个 UI 都先经它折叠平台批次，再各自
维护视图状态；GPUI 不保留第二份数据 fold。

**求同**（单一实现，两轨共享，漂移由门禁抓死）：

1. application 层 typed request/event 契约与 provider lane 是两轨唯一语义源；
2. 命令/模态/批量 outcome 语义（EndTask/Signal/Affinity/ResourceLimits 的 typed
   反馈）共享；
3. 页表单一来源：`taskmanager-application::AppPage::ALL` + `PageDescriptor`；
4. renderer-neutral 词汇（help/建议/过滤、武装确认门 `y`/`n`，armed 门拥有键盘并
   吞掉其余字符）一律上提 shell，GPUI 按原样消费、不重新解词；
5. 行为对等由共享事件轨迹持续锁定——policy/alert 可保留两轨实例并逐步断言一致；
   live graph history 则统一读取 `taskmanager-telemetry-store` 的 generation/gap/revision
   语义，GPUI 只保留 revision-keyed renderer cache，不再拥有第二套事实权威。

**存异**（有意分工，各自优势所在）：

| 轨 | 自有状态/路径 | 优势 |
|---|---|---|
| GPUI 直连轨 | 数据折叠经共享 `SystemProjectionStore`；`RootView` 保留 per-window `!Send` Entity、视图镜像与多窗口槽位（`cached`/`procs` 等视图字段只由 typed change report 物化） | Entity 粒度 `cx.notify()` 精细失效（渲染门与跨帧场景缓存建在其上）、gpui 交互栈全量能力、每窗口隔离 |
| shell 轨（tui/iced） | `ShellApp` 状态机 + `queue_effect` 单一提交路径（直连提交被 `frontend_submission_ownership_test` 结构性禁止） | 两个轻前端共享一份可无头穷测的纯状态机，不各自复制 reducer |

**有意的双词表/三通道**：shell `SortCol`（含 Pss/State）与 GPUI `SortCol`（含
Status/StartTime/Fds、无 Pss）是两个前端族的显示词表，按前端族各自演进；传感器
scalar 快照、per-field provenance 与滚动历史三通道保持分离，消费端不合并。alerts 阈值建议面（G-17）三
前端均无表面，成文推迟——恢复时需三前端一致或显式豁免。

**依赖边界**：shell 只依赖 `taskmanager-application`、`taskmanager-telemetry-store` 与
`taskmanager-ui-contract`；
不得依赖 theme、ratatui、gpui、iced、platform adapter 或文件系统/命令 I/O。主题
数据留在 renderer 边缘（ADR-026 neutral registry 除外）。renderer 专属扩展（如
GPUI 的 Containers 页）在六页契约之外。

## Consequences

- 命令与模态语义一份状态机实现；renderer 测试只断言像素/终端单元。
- 平台批次数据折叠一份实现（`SystemProjectionStore`）；新增 capability 只改 store 一处，
  `shared_projection_fold_test` 结构门禁止 UI crate 再出现原始 batch fold 臂。
- TUI/iced 的 shell 消费以行为套件全绿为准；GPUI 不得声称"已 shell 化"，其采纳
  形态就是共享数据折叠 + 直连视图轨 + parity 门禁。
- 新增 neutral 词汇上提 shell；toolkit 类型永不进入 shell。
- 本决策只定义共享 shell 边界；产品能力是否可用由对应领域合同、失败语义和目标平台
  验证共同决定。
